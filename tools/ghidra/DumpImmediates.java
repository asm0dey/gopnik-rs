import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.address.SegmentedAddress;
import ghidra.program.model.listing.Instruction;
import ghidra.program.model.listing.InstructionIterator;
import ghidra.program.model.scalar.Scalar;

import java.io.File;
import java.io.PrintWriter;
import java.nio.file.Files;
import java.nio.file.Paths;
import java.util.ArrayList;
import java.util.List;
import java.util.TreeMap;
import java.util.TreeSet;

/**
 * Recovers string-constant file offsets from genuine *immediate* operands in
 * Ghidra's real disassembly, as opposed to a naive byte-pattern scan for
 * opcodes like BA/B8/68.
 *
 * Address mapping (see docs/re/string-pointers.md): the program image begins
 * at file offset 0x18D0 and loads at segment 0x1000. Every memory block in
 * this import is part of one contiguous flat image, so for an instruction at
 * segment S, a 16-bit value imm used as a near offset within that same
 * segment maps to file_offset = 0x18D0 + (S - 0x1000) * 16 + imm. This
 * generalizes "file_offset = 0x18D0 + imm" (the rule for S == 0x1000, i.e.
 * CODE_0) to instructions living in any other code block.
 *
 * Operand extraction (round-2 fix): this uses Instruction.getScalar(opIndex),
 * NOT Instruction.getOpObjects(opIndex). getScalar returns non-null only when
 * an operand decomposes to exactly one object and that object is a Scalar -
 * i.e. the operand IS an immediate, not a memory-addressing expression built
 * from a register plus a displacement. Walking getOpObjects instead (the
 * round-1 approach) decomposed expressions like "[BP + 0x4]" and
 * "word ptr [0x38C5]" into their component scalars/addresses and treated
 * stack-frame displacements and unrelated global-variable addresses as
 * string-pointer candidates - producing false positives such as the
 * 0x18D0-0x18DA displacement-artefact run and the 0x5195 misframed
 * mid-string offset. getScalar is applied here to every operand of every
 * instruction regardless of mnemonic (MOV/PUSH/CMP/ADD/... all appear in the
 * audit trail) - the breadth requirement is about not restricting to
 * MOV/PUSH, never about accepting address-expression arithmetic.
 *
 * Interior-offset rejection: after the content filter, any candidate that
 * falls strictly inside an already-accepted candidate's payload span (i.e.
 * it is a byte of another string's text, not a string start in its own
 * right) is rejected. That is the same space-misread-as-length-byte
 * pathology this task exists to eliminate, arriving through a different
 * door: two independent immediate operands can coincidentally land on the
 * same constant pool region, one at the genuine string start and another a
 * few bytes into its payload where a content byte happens to look like a
 * valid length prefix.
 */
public class DumpImmediates extends GhidraScript {

    private static final long IMAGE_FILE_OFFSET = 0x18D0L;
    private static final int BASE_SEGMENT = 0x1000;

    private static class Hit {
        String addr;
        String mnemonic;
        int opIndex;
        long imm;
        long fileOffset;
        boolean inBounds;
        String instrText;

        Hit(String addr, String mnemonic, int opIndex, long imm, long fileOffset,
                boolean inBounds, String instrText) {
            this.addr = addr;
            this.mnemonic = mnemonic;
            this.opIndex = opIndex;
            this.imm = imm;
            this.fileOffset = fileOffset;
            this.inBounds = inBounds;
            this.instrText = instrText;
        }
    }

    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        String outDir = args.length > 0 ? args[0] : "build";
        String origExePath = args.length > 1 ? args[1] : "orig/g.exe";

        byte[] blob = Files.readAllBytes(Paths.get(origExePath));

        File dir = new File(outDir);
        dir.mkdirs();

        List<Hit> hits = new ArrayList<>();
        int totalHits = 0;
        int lesLdsInstructions = 0;

        InstructionIterator it = currentProgram.getListing().getInstructions(true);
        while (it.hasNext()) {
            Instruction instr = it.next();
            Address addr = instr.getMinAddress();
            int segment = BASE_SEGMENT;
            if (addr instanceof SegmentedAddress) {
                segment = ((SegmentedAddress) addr).getSegment();
            }

            String mnemonic = instr.getMnemonicString();
            if (mnemonic.equals("LES") || mnemonic.equals("LDS")) {
                lesLdsInstructions++;
            }

            int numOps = instr.getNumOperands();
            for (int opIndex = 0; opIndex < numOps; opIndex++) {
                // getScalar returns non-null only when the operand IS a bare
                // immediate scalar (getOpObjects for that operand has
                // exactly one element and it is a Scalar). Memory-addressing
                // expressions such as "[BP + 0x4]" or "[SI + 0x2]" decompose
                // to more than one op-object (a register plus a scalar) and
                // yield null here, which is the point: those scalars are
                // displacements/indices, not string addresses.
                Scalar sc = instr.getScalar(opIndex);
                if (sc == null) {
                    continue;
                }
                if (sc.bitLength() != 16) {
                    continue;
                }
                long v = sc.getUnsignedValue();
                if (v < 0x0000L || v > 0xFFFFL) {
                    continue;
                }

                totalHits++;
                long fileOffset = IMAGE_FILE_OFFSET + ((long) (segment - BASE_SEGMENT) * 16L) + v;
                boolean inBounds = fileOffset >= 0 && fileOffset < blob.length;
                hits.add(new Hit(addr.toString(), mnemonic, opIndex, v, fileOffset, inBounds,
                        instr.toString().replace("\t", " ")));
            }
        }

        // Pass 2: content filter - which offsets are well-formed shortstring starts.
        TreeSet<Long> contentAccepted = new TreeSet<>();
        for (Hit h : hits) {
            if (h.inBounds && isGenuineStringStart(blob, (int) h.fileOffset)) {
                contentAccepted.add(h.fileOffset);
            }
        }

        // Pass 3: interior-offset rejection. Reject any content-accepted
        // offset that lies strictly inside another *genuine* offset's
        // payload span - it is a byte of that other string, not an
        // independent string start. "Genuine" must mean already-confirmed
        // (kept), not merely content-filter-accepted: a misframed candidate
        // that itself sits inside a real string's payload (the classic
        // space-as-length-byte collision) must not be allowed to cover and
        // reject the real, distinct string that follows it. Because any
        // interior offset is by definition greater than its container's
        // offset, a single ascending pass that only checks each candidate
        // against previously-kept (already-validated) candidates resolves
        // this correctly without transitive false rejection.
        TreeMap<Long, Long> interiorRejected = new TreeMap<>(); // offset -> covering offset
        TreeSet<Long> accepted = new TreeSet<>();
        for (long off : contentAccepted) {
            long coveringOffset = -1;
            for (long kept : accepted) {
                int n = blob[(int) kept] & 0xFF;
                if (kept < off && off < kept + 1 + n) {
                    coveringOffset = kept;
                    break;
                }
            }
            if (coveringOffset >= 0) {
                interiorRejected.put(off, coveringOffset);
            } else {
                accepted.add(off);
            }
        }

        // Pass 4: emit the audit trail with per-row status and an
        // acceptance-reason column (mnemonic + operand kind) so a future
        // regression - e.g. accidentally accepting a non-immediate operand
        // again - is visible directly in the committed artifact.
        List<String> lines = new ArrayList<>();
        for (Hit h : hits) {
            String status;
            if (!h.inBounds) {
                status = "reject:out-of-bounds";
            } else if (!contentAccepted.contains(h.fileOffset)) {
                status = "reject:not-a-string";
            } else if (interiorRejected.containsKey(h.fileOffset)) {
                status = String.format("reject:interior:0x%X", interiorRejected.get(h.fileOffset));
            } else {
                status = "KEEP";
            }
            String reason = h.mnemonic + "/immediate-scalar";
            String text = String.format("%s\t%s\top%d\t0x%04X\t%s\t%s\t%s\t%s",
                    h.addr, h.mnemonic, h.opIndex, h.imm,
                    h.inBounds ? String.format("0x%X", h.fileOffset) : "-",
                    h.instrText, status, reason);
            lines.add(text);
        }

        File auditFile = new File(dir, "string_pointers_audit.tsv");
        try (PrintWriter pw = new PrintWriter(auditFile, "UTF-8")) {
            pw.println("address\tmnemonic\toperand\timmediate\tfile_offset\tinstruction\tstatus\treason");
            for (String line : lines) {
                pw.println(line);
            }
        }

        File jf = new File(dir, "string_pointers.json");
        try (PrintWriter pw = new PrintWriter(jf, "UTF-8")) {
            pw.println("{");
            pw.println("  \"note\": \"File offsets into orig/g.exe recovered from Instruction.getScalar "
                    + "(bare immediate operands only, no memory-expression decomposition) across every "
                    + "instruction in Ghidra's real disassembly regardless of mnemonic, mapped via "
                    + "file_offset = 0x18D0 + (segment - 0x1000) * 16 + imm, filtered to offsets whose byte "
                    + "is a well-formed Pascal shortstring length (3..250) with an in-bounds, "
                    + "CP866/ASCII/0x07/0x0A/0x0D-only payload, and then to offsets that do not fall inside "
                    + "another accepted offset's payload span. No mnemonic restriction and no "
                    + "reuse/reference-count filter is applied - see docs/re/string-pointers.md.\",");
            StringBuilder sb = new StringBuilder();
            sb.append("  \"pointers\": [");
            boolean first = true;
            for (long off : accepted) {
                if (!first) {
                    sb.append(", ");
                }
                sb.append(off);
                first = false;
            }
            sb.append("]");
            pw.println(sb.toString());
            pw.println("}");
        }

        println("DumpImmediates: instructions=" + currentProgram.getListing().getNumInstructions()
                + " scalar-hits=" + totalHits
                + " content-accepted=" + contentAccepted.size()
                + " interior-rejected=" + interiorRejected.size()
                + " accepted=" + accepted.size()
                + " LES/LDS-instructions=" + lesLdsInstructions);
    }

    private boolean isGenuineStringStart(byte[] blob, int off) {
        if (off < 0 || off >= blob.length) {
            return false;
        }
        int n = blob[off] & 0xFF;
        if (n < 3 || n > 250) {
            return false;
        }
        if (off + 1 + n > blob.length) {
            return false;
        }
        for (int i = 0; i < n; i++) {
            int b = blob[off + 1 + i] & 0xFF;
            boolean ok = (b >= 0x20 && b <= 0x7E) || (b >= 0x80 && b <= 0xF1)
                    || b == 0x07 || b == 0x0A || b == 0x0D;
            if (!ok) {
                return false;
            }
        }
        return true;
    }
}
