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
import java.util.TreeSet;

/**
 * Recovers string-constant file offsets from the 16-bit scalar operands that
 * Ghidra's real disassembly actually attaches to instructions, as opposed to
 * a naive byte-pattern scan for opcodes like BA/B8/68.
 *
 * Address mapping (see docs/re/string-pointers.md): the program image begins
 * at file offset 0x18D0 and loads at segment 0x1000. Every memory block in
 * this import is part of one contiguous flat image, so for an instruction at
 * segment S, a 16-bit value imm used as a near offset within that same
 * segment maps to file_offset = 0x18D0 + (S - 0x1000) * 16 + imm. This
 * generalizes "file_offset = 0x18D0 + imm" (the rule for S == 0x1000, i.e.
 * CODE_0) to instructions living in any other code block, per the brief's
 * instruction to derive the base from the containing memory block rather
 * than assuming 0x1000 unconditionally.
 *
 * Every instruction's operands are considered, regardless of mnemonic
 * (MOV/PUSH/LES/LDS/and everything else) — reference count is not used as
 * evidence either way. For each operand this walks Instruction.getOpObjects
 * rather than relying solely on Instruction.getScalar: Ghidra's own
 * auto-analysis (run at import time, before this script runs with
 * -noanalysis) frequently recognizes an immediate that looks like a pointer
 * and rewrites the operand as an Address rather than leaving it a plain
 * Scalar — this happens routinely for PUSH <addr> and for LES/LDS
 * far-pointer loads. getScalar() returns null for those operands, so a
 * scalar-only scan silently drops every string address Ghidra already
 * recognized as an address. Both Scalar and Address operand objects are
 * handled here so no addressing form is skipped. Only the content filter
 * below (a well-formed Pascal shortstring at the candidate offset) decides
 * what qualifies as a string pointer.
 */
public class DumpImmediates extends GhidraScript {

    private static final long IMAGE_FILE_OFFSET = 0x18D0L;
    private static final int BASE_SEGMENT = 0x1000;

    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        String outDir = args.length > 0 ? args[0] : "build";
        String origExePath = args.length > 1 ? args[1] : "orig/g.exe";

        byte[] blob = Files.readAllBytes(Paths.get(origExePath));

        File dir = new File(outDir);
        dir.mkdirs();

        List<String> hitText = new ArrayList<>();
        List<String> rejectedLines = new ArrayList<>();
        TreeSet<Long> accepted = new TreeSet<>();
        int totalHits = 0;

        InstructionIterator it = currentProgram.getListing().getInstructions(true);
        while (it.hasNext()) {
            Instruction instr = it.next();
            Address addr = instr.getMinAddress();
            int segment = BASE_SEGMENT;
            if (addr instanceof SegmentedAddress) {
                segment = ((SegmentedAddress) addr).getSegment();
            }

            String mnemonic = instr.getMnemonicString();

            int numOps = instr.getNumOperands();
            for (int opIndex = 0; opIndex < numOps; opIndex++) {
                // Ghidra's own auto-analysis (run during the initial import,
                // before this script runs with -noanalysis) frequently
                // converts an immediate that looks like a pointer into an
                // Address-typed operand rather than leaving it as a plain
                // Scalar - this is common for PUSH <addr> and for LES/LDS
                // far-pointer loads in particular. Instruction.getScalar()
                // returns null for such operands, so relying on it alone
                // silently drops every string address that Ghidra already
                // recognized as an address. To see every candidate
                // regardless of how Ghidra represented it, walk the
                // operand's underlying objects (Instruction.getOpObjects)
                // and handle both Scalar and Address forms.
                for (Object obj : instr.getOpObjects(opIndex)) {
                    Long imm = null;
                    int objSegment = segment;
                    if (obj instanceof Scalar) {
                        Scalar sc = (Scalar) obj;
                        if (sc.bitLength() != 16) {
                            continue;
                        }
                        long v = sc.getUnsignedValue();
                        if (v < 0x0000L || v > 0xFFFFL) {
                            continue;
                        }
                        imm = v;
                    } else if (obj instanceof Address) {
                        Address a = (Address) obj;
                        if (a instanceof SegmentedAddress) {
                            objSegment = ((SegmentedAddress) a).getSegment();
                        }
                        imm = a.getOffset() & 0xFFFFL;
                    } else {
                        continue;
                    }

                    totalHits++;
                    long fileOffset = IMAGE_FILE_OFFSET + ((long) (objSegment - BASE_SEGMENT) * 16L) + imm;
                    boolean inBounds = fileOffset >= 0 && fileOffset < blob.length;
                    String text = String.format("%s\t%s\top%d\t0x%04X\t%s\t%s",
                            addr.toString(), mnemonic, opIndex, imm,
                            inBounds ? String.format("0x%X", fileOffset) : "-",
                            instr.toString().replace("\t", " "));

                    if (!inBounds) {
                        rejectedLines.add(text + "\treject:out-of-bounds");
                        continue;
                    }
                    if (!isGenuineStringStart(blob, (int) fileOffset)) {
                        rejectedLines.add(text + "\treject:not-a-string");
                        continue;
                    }

                    accepted.add(fileOffset);
                    hitText.add(text + "\tKEEP");
                }
            }
        }

        File auditFile = new File(dir, "string_pointers_audit.tsv");
        try (PrintWriter pw = new PrintWriter(auditFile, "UTF-8")) {
            pw.println("address\tmnemonic\toperand\timmediate\tfile_offset\tinstruction\tstatus");
            for (String line : hitText) {
                pw.println(line);
            }
            for (String line : rejectedLines) {
                pw.println(line);
            }
        }

        File jf = new File(dir, "string_pointers.json");
        try (PrintWriter pw = new PrintWriter(jf, "UTF-8")) {
            pw.println("{");
            pw.println("  \"note\": \"File offsets into orig/g.exe recovered from the 16-bit "
                    + "scalar and address operand objects of every instruction in Ghidra's "
                    + "real disassembly (Instruction.getOpObjects), mapped via file_offset = "
                    + "0x18D0 + (segment - 0x1000) * 16 + imm, filtered to offsets whose byte "
                    + "is a well-formed Pascal shortstring length (3..250) with an in-bounds, "
                    + "CP866/ASCII/0x07/0x0A/0x0D-only payload. No mnemonic restriction and no "
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
                + " accepted=" + accepted.size());
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
