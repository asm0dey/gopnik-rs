import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.address.AddressSetView;
import ghidra.program.model.address.SegmentedAddress;
import ghidra.program.model.lang.Register;
import ghidra.program.model.listing.CodeUnit;
import ghidra.program.model.listing.Data;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.FunctionManager;
import ghidra.program.model.listing.Instruction;
import ghidra.program.model.listing.Listing;
import ghidra.program.model.mem.Memory;
import ghidra.program.model.mem.MemoryBlock;
import ghidra.program.model.symbol.FlowType;
import ghidra.program.model.symbol.RefType;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceManager;

import java.io.File;
import java.io.PrintWriter;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.HashSet;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.regex.Matcher;
import java.util.regex.Pattern;
import java.util.stream.Collectors;
import java.util.stream.Stream;

/**
 * Enumerate every conditional branch Ghidra found inside an identified function,
 * with the flag-setting instruction that guards it, a game/RTL classification,
 * and a citation-based cross-reference against the Rust port.
 *
 * Usage (see tools/ghidra/run_ghidra.sh):
 *   -postScript EnumerateBranches.java <outDir> <repoRoot>
 *
 * Writes <outDir>/branches.json. The script asserts nothing about code Ghidra
 * did not disassemble; the `limits` object in the output quantifies exactly how
 * much of the image that is.
 *
 * Address convention: Ghidra loads the image at segment 0x1000, so a Ghidra
 * address SEG:OFF maps to file offset 0x18d0 + (SEG - 0x1000) * 16 + OFF, and
 * the real DOS segment is SEG - 0x1000. Every emitted branch carries its raw
 * opcode bytes AND the file offset computed both ways (arithmetic and Ghidra's
 * own file-byte mapping) so a drift is caught rather than propagated.
 */
public class EnumerateBranches extends GhidraScript {

    /** Header size of orig/g.exe: the MZ header is not loaded into memory. */
    private static final long IMAGE_BASE_FILE_OFF = 0x18d0L;
    private static final int GHIDRA_BASE_SEGMENT = 0x1000;

    /** How far back to look for the instruction that set the consumed flags. */
    private static final int GUARD_SCAN_LIMIT = 12;

    private static final Set<String> FLAG_REGS =
            new HashSet<>(Arrays.asList("CF", "ZF", "SF", "OF", "PF", "AF"));

    private static final Pattern CITATION =
            Pattern.compile("\\b([0-9a-fA-F]{4}):([0-9a-fA-F]{1,4})\\b");

    private Listing listing;
    private FunctionManager fm;
    private ReferenceManager rm;
    private Memory memory;

    // ------------------------------------------------------------------ main

    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        String outDir = args.length > 0 ? args[0] : "build";
        String repoRoot = args.length > 1 ? args[1] : ".";

        listing = currentProgram.getListing();
        fm = currentProgram.getFunctionManager();
        rm = currentProgram.getReferenceManager();
        memory = currentProgram.getMemory();

        Map<Address, List<String>> citations = scanPortCitations(repoRoot);

        List<Function> funcs = new ArrayList<>();
        for (Function f : fm.getFunctions(true)) {
            funcs.add(f);
        }

        Map<String, Integer> branchCount = new HashMap<>();
        Map<String, Integer> citedBranchCount = new HashMap<>();
        List<String> branchJson = new ArrayList<>();
        List<String> indirectJson = new ArrayList<>();

        int totalBranches = 0;
        int indirectJumps = 0;
        int indirectCalls = 0;
        int fileOffMismatches = 0;
        int guardResolved = 0;
        int guardNull = 0;
        int guardJoinCrossed = 0;

        for (Function f : funcs) {
            String fname = f.getName();
            String fclass = classify(f);
            branchCount.put(fname, 0);
            citedBranchCount.put(fname, 0);

            AddressSetView body = f.getBody();
            for (Instruction ins : listing.getInstructions(body, true)) {
                FlowType ft = ins.getFlowType();
                if (ft == null) {
                    continue;
                }

                if (!ft.isConditional() || !ft.isJump()) {
                    continue;
                }

                totalBranches++;
                branchCount.merge(fname, 1, Integer::sum);
                if (!Long.toString(fileOffCalc(ins.getAddress()))
                        .equals(fileOffGhidra(ins.getAddress()))) {
                    fileOffMismatches++;
                    println("FILE OFFSET MISMATCH at " + ins.getAddress()
                            + " calc=" + fileOffCalc(ins.getAddress())
                            + " ghidra=" + fileOffGhidra(ins.getAddress()));
                }

                Guard g = resolveGuard(ins, body);
                if (g.insn == null) {
                    guardNull++;
                } else {
                    guardResolved++;
                    if (g.joinCrossed) {
                        guardJoinCrossed++;
                    }
                }

                Address[] flows = ins.getFlows();
                Address taken = flows.length > 0 ? flows[0] : null;
                Address fall = ins.getFallThrough();

                List<String> cited = citations.getOrDefault(ins.getAddress(), null);
                List<String> guardCited = g.insn == null ? null
                        : citations.getOrDefault(g.insn.getAddress(), null);
                boolean touched = cited != null || guardCited != null;
                if (touched) {
                    citedBranchCount.merge(fname, 1, Integer::sum);
                }
                long nearest = nearestCitation(ins.getAddress(), f, citations);

                StringBuilder sb = new StringBuilder();
                sb.append("  {\"addr\": ").append(q(addr(ins.getAddress())));
                sb.append(", \"file_off\": ").append(fileOffCalc(ins.getAddress()));
                sb.append(", \"file_off_ghidra\": ").append(fileOffGhidra(ins.getAddress()));
                sb.append(", \"real_seg_off\": ").append(q(realSegOff(ins.getAddress())));
                sb.append(", \"bytes\": ").append(q(hex(ins)));
                sb.append(", \"func\": ").append(q(fname));
                sb.append(", \"func_entry\": ").append(q(addr(f.getEntryPoint())));
                sb.append(", \"class\": ").append(q(fclass));
                sb.append(", \"mnemonic\": ").append(q(ins.getMnemonicString()));
                sb.append(", \"text\": ").append(q(ins.toString()));
                sb.append(", \"taken\": ").append(taken == null ? "null" : q(addr(taken)));
                sb.append(", \"fallthrough\": ").append(fall == null ? "null" : q(addr(fall)));
                sb.append(", \"reads_flags\": [").append(qJoin(new ArrayList<>(g.wanted))).append("]");
                if (g.insn == null) {
                    sb.append(", \"guard\": null");
                    sb.append(", \"guard_status\": ").append(q(g.status));
                    sb.append(", \"guard_flag_source_call\": ").append(q(g.flagSourceCall));
                } else {
                    sb.append(", \"guard\": {");
                    sb.append("\"addr\": ").append(q(addr(g.insn.getAddress())));
                    sb.append(", \"file_off\": ").append(fileOffCalc(g.insn.getAddress()));
                    sb.append(", \"bytes\": ").append(q(hex(g.insn)));
                    sb.append(", \"mnemonic\": ").append(q(g.insn.getMnemonicString()));
                    sb.append(", \"text\": ").append(q(g.insn.toString()));
                    sb.append(", \"distance\": ").append(g.distance);
                    sb.append(", \"kind\": ").append(q(g.kind));
                    sb.append(", \"join_crossed\": ").append(g.joinCrossed);
                    sb.append(", \"shared_with_preceding_branch\": ")
                            .append(g.sharedWithPrecedingBranch);
                    sb.append("}");
                    sb.append(", \"guard_status\": ").append(q(g.status));
                }
                sb.append(", \"cited_in_port\": ").append(cited != null);
                sb.append(", \"guard_cited_in_port\": ").append(guardCited != null);
                sb.append(", \"port_touched\": ").append(touched);
                sb.append(", \"bytes_to_nearest_port_citation\": ")
                        .append(nearest < 0 ? "null" : Long.toString(nearest));
                sb.append(", \"port_citations\": [")
                        .append(cited == null ? "" : qJoin(cited)).append("]");
                sb.append(", \"guard_port_citations\": [")
                        .append(guardCited == null ? "" : qJoin(guardCited)).append("]");
                sb.append("}");
                branchJson.add(sb.toString());
            }
        }

        // ------------------------------------------------------------ functions
        List<String> funcJson = new ArrayList<>();
        for (Function f : funcs) {
            String fname = f.getName();
            String fclass = classify(f);
            Set<String> fcit = new LinkedHashSet<>();
            for (Address a : f.getBody().getAddresses(true)) {
                List<String> c = citations.get(a);
                if (c != null) {
                    fcit.addAll(c);
                }
            }
            int callers = 0;
            Set<String> seen = new HashSet<>();
            for (Reference r : rm.getReferencesTo(f.getEntryPoint())) {
                Function cf = fm.getFunctionContaining(r.getFromAddress());
                if (cf != null && seen.add(cf.getName())) {
                    callers++;
                }
            }
            boolean callsGameSeg = false;
            for (Function cf : f.getCalledFunctions(monitor)) {
                if (segOf(cf.getEntryPoint()) == GHIDRA_BASE_SEGMENT) {
                    callsGameSeg = true;
                }
            }

            StringBuilder sb = new StringBuilder();
            sb.append("  {\"name\": ").append(q(fname));
            sb.append(", \"entry\": ").append(q(addr(f.getEntryPoint())));
            sb.append(", \"seg\": ").append(q(String.format("%04x", segOf(f.getEntryPoint()))));
            sb.append(", \"real_seg\": ")
                    .append(q(String.format("%04x", segOf(f.getEntryPoint()) - GHIDRA_BASE_SEGMENT)));
            sb.append(", \"entry_file_off\": ").append(fileOffCalc(f.getEntryPoint()));
            sb.append(", \"size\": ").append(f.getBody().getNumAddresses());
            sb.append(", \"class\": ").append(q(fclass));
            sb.append(", \"caller_count\": ").append(callers);
            sb.append(", \"calls_segment_1000\": ").append(callsGameSeg);
            sb.append(", \"branch_count\": ").append(branchCount.getOrDefault(fname, 0));
            sb.append(", \"branches_touched_by_port\": ")
                    .append(citedBranchCount.getOrDefault(fname, 0));
            sb.append(", \"port_citation_count\": ").append(fcit.size());
            sb.append(", \"cited_in_port\": ").append(!fcit.isEmpty());
            sb.append(", \"port_citations\": [").append(qJoin(new ArrayList<>(fcit))).append("]");
            sb.append("}");
            funcJson.add(sb.toString());
        }

        // ------------------------------------------------ uncited spans
        // A span is a maximal address interval inside a game function that no
        // port citation falls in. Spans holding many branches are the parts of
        // the original the port has said nothing about, ranked mechanically.
        List<String> spanJson = new ArrayList<>();
        for (Function f : funcs) {
            if (!"game".equals(classify(f))) {
                continue;
            }
            List<Long> cuts = new ArrayList<>();
            List<Long> brs = new ArrayList<>();
            long lo = f.getEntryPoint().getOffset();
            long hi = lo + f.getBody().getNumAddresses();
            for (Address a : f.getBody().getAddresses(true)) {
                if (citations.containsKey(a)) {
                    cuts.add(a.getOffset());
                }
            }
            for (Instruction ins : listing.getInstructions(f.getBody(), true)) {
                FlowType ft = ins.getFlowType();
                if (ft != null && ft.isConditional() && ft.isJump()) {
                    brs.add(ins.getAddress().getOffset());
                }
            }
            cuts.sort(Long::compare);
            long spanLo = lo;
            List<Long> bounds = new ArrayList<>(cuts);
            bounds.add(hi);
            for (long cut : bounds) {
                List<Long> inSpan = new ArrayList<>();
                for (long b : brs) {
                    if (b >= spanLo && b < cut) {
                        inSpan.add(b);
                    }
                }
                if (!inSpan.isEmpty()) {
                    Address sa = f.getEntryPoint().getNewAddress(spanLo);
                    Address ea = f.getEntryPoint().getNewAddress(cut - 1);
                    List<String> baddrs = new ArrayList<>();
                    for (long b : inSpan) {
                        baddrs.add(addr(f.getEntryPoint().getNewAddress(b)));
                    }
                    spanJson.add("  {\"func\": " + q(f.getName())
                            + ", \"start\": " + q(addr(sa))
                            + ", \"end\": " + q(addr(ea))
                            + ", \"bytes\": " + (cut - spanLo)
                            + ", \"branch_count\": " + inSpan.size()
                            + ", \"branches\": [" + qJoin(baddrs) + "]}");
                }
                spanLo = cut + 1;
            }
        }

        // ------------------------------------- program-wide indirect control flow
        // Not restricted to function bodies: a jump table that Ghidra could not
        // follow is exactly the case where the surrounding code may be outside
        // any function.
        for (Instruction ins : listing.getInstructions(true)) {
            FlowType ft = ins.getFlowType();
            if (ft == null || !(ft.isJump() || ft.isCall())) {
                continue;
            }
            boolean unresolved = ft.isComputed() || ins.getFlows().length == 0;
            if (!unresolved) {
                continue;
            }
            String kind;
            if ("INT".equals(ins.getMnemonicString())) {
                kind = "software_interrupt";
            } else if (ft.isCall()) {
                kind = "indirect_call";
                indirectCalls++;
            } else {
                kind = "indirect_jump";
                indirectJumps++;
            }
            Function cf = fm.getFunctionContaining(ins.getAddress());
            indirectJson.add(indirectRecord(ins, cf,
                    cf == null ? "outside_function" : classify(cf),
                    ft.isConditional() ? "conditional" : "unconditional", kind));
        }

        // --------------------------------------------------------------- limits
        long loadedBytes = 0;
        long codeBlockBytes = 0;
        long codeBlockFuncBytes = 0;
        List<String> blockJson = new ArrayList<>();
        List<String> gapJson = new ArrayList<>();
        for (MemoryBlock b : memory.getBlocks()) {
            if (b.isInitialized()) {
                loadedBytes += b.getSize();
            }
            long inFunc = 0;
            long inInsn = 0;
            long inData = 0;
            long undef = 0;
            // Every contiguous run Ghidra left undefined inside this block.
            List<long[]> gaps = new ArrayList<>();
            long gapStart = -1;
            Address a = b.getStart();
            while (a != null && a.compareTo(b.getEnd()) <= 0) {
                CodeUnit cu = listing.getCodeUnitAt(a);
                long len = 1;
                boolean defined = false;
                if (cu instanceof Instruction) {
                    len = cu.getLength();
                    inInsn += len;
                    defined = true;
                    if (fm.getFunctionContaining(a) != null) {
                        inFunc += len;
                    }
                } else if (cu instanceof Data && ((Data) cu).isDefined()) {
                    len = cu.getLength();
                    inData += len;
                    defined = true;
                } else {
                    if (cu != null) {
                        len = Math.max(1, cu.getLength());
                    }
                    undef += len;
                }
                // Offsets are taken relative to the block start: adding to a
                // SegmentedAddress can renormalise its segment, so the in-segment
                // offset of a derived address is not a stable coordinate.
                long rel = a.subtract(b.getStart());
                if (!defined && gapStart < 0) {
                    gapStart = rel;
                } else if (defined && gapStart >= 0) {
                    gaps.add(new long[]{gapStart, rel - gapStart});
                    gapStart = -1;
                }
                try {
                    a = a.addNoWrap(len);
                } catch (Exception e) {
                    a = null;
                }
                if (a != null && a.compareTo(b.getEnd()) > 0) {
                    break;
                }
            }
            if (gapStart >= 0) {
                gaps.add(new long[]{gapStart, b.getSize() - gapStart});
            }
            long gapBytes = 0;
            for (long[] gp : gaps) {
                gapBytes += gp[1];
            }
            if (gapBytes != undef) {
                println("BLOCK GAP ACCOUNTING MISMATCH " + b.getName()
                        + " runs=" + gapBytes + " undefined=" + undef);
            }
            boolean isCodeBlock = b.getName().startsWith("CODE_") && inInsn > 0;
            if (isCodeBlock) {
                codeBlockBytes += b.getSize();
                codeBlockFuncBytes += inFunc;
            }
            gaps.sort((x, y) -> Long.compare(y[1], x[1]));
            for (int i = 0; i < gaps.size(); i++) {
                gapJson.add("    {\"block\": " + q(b.getName())
                        + ", \"start\": " + q(String.format("%04x:%04x",
                                segOf(b.getStart()), offOf(b.getStart()) + gaps.get(i)[0]))
                        + ", \"file_off\": " + (fileOffCalc(b.getStart()) + gaps.get(i)[0])
                        + ", \"length\": " + gaps.get(i)[1] + "}");
            }
            blockJson.add("    {\"name\": " + q(b.getName())
                    + ", \"start\": " + q(addr(b.getStart()))
                    + ", \"end\": " + q(addr(b.getEnd()))
                    + ", \"size\": " + b.getSize()
                    + ", \"initialized\": " + b.isInitialized()
                    + ", \"counted_as_code_block\": " + isCodeBlock
                    + ", \"bytes_in_functions\": " + inFunc
                    + ", \"bytes_as_instructions\": " + inInsn
                    + ", \"bytes_as_defined_data\": " + inData
                    + ", \"bytes_undefined\": " + undef
                    + ", \"undefined_runs\": " + gaps.size() + "}");
        }

        long funcBytes = 0;
        long gameFuncBytes = 0;
        for (Function f : funcs) {
            funcBytes += f.getBody().getNumAddresses();
            if ("game".equals(classify(f))) {
                gameFuncBytes += f.getBody().getNumAddresses();
            }
        }

        long insnBytes = 0;
        long insnCount = 0;
        long insnBytesOutsideFunctions = 0;
        for (Instruction ins : listing.getInstructions(true)) {
            insnBytes += ins.getLength();
            insnCount++;
            if (fm.getFunctionContaining(ins.getAddress()) == null) {
                insnBytesOutsideFunctions += ins.getLength();
            }
        }
        long dataBytes = 0;
        for (Data d : listing.getDefinedData(true)) {
            dataBytes += d.getLength();
        }
        long undefinedBytes = loadedBytes - insnBytes - dataBytes;

        long fileBytes = new File(repoRoot, "orig/g.exe").length();

        // ---------------------------------------------------------------- emit
        new File(outDir).mkdirs();
        File jf = new File(outDir, "branches.json");
        try (PrintWriter pw = new PrintWriter(jf, "UTF-8")) {
            pw.println("{");
            pw.println("\"schema_version\": 1,");
            pw.println("\"generator\": \"tools/ghidra/EnumerateBranches.java\",");
            pw.println("\"program\": " + q(currentProgram.getName()) + ",");
            pw.println("\"ghidra_version\": "
                    + q(ghidra.framework.Application.getApplicationVersion()) + ",");

            pw.println("\"address_convention\": {");
            pw.println("  \"note\": \"Addresses are Ghidra segmented addresses. Ghidra loads the"
                    + " image at segment 0x1000, so the real DOS segment is SEG-0x1000.\",");
            pw.println("  \"file_off_formula\": \"0x18d0 + (SEG - 0x1000) * 16 + OFF\",");
            pw.println("  \"file_off_ghidra\": \"Memory.getAddressSourceInfo().getFileOffset(),"
                    + " independent of the formula; the two must agree.\"");
            pw.println("},");

            pw.println("\"classification_rule\": {");
            pw.println("  \"rule\": \"game iff the function's Ghidra segment == 0x1000 (real DOS"
                    + " segment 0x0000, the program's own code segment); rtl otherwise.\",");
            pw.println("  \"basis\": \"Borland Pascal emits the main program body into its own"
                    + " code segment and each linked unit (System, Crt, Dos, Overlay) into"
                    + " further segments. Ghidra's MZ loader gives each a separate block.\",");
            pw.println("  \"heuristic\": true,");
            pw.println("  \"rerun\": \"Every function record carries seg, real_seg, caller_count"
                    + " and calls_segment_1000, so a different rule can be applied to this file"
                    + " without re-running Ghidra. No branch is dropped by classification.\"");
            pw.println("},");

            pw.println("\"limits\": {");
            pw.println("  \"file_bytes\": " + fileBytes + ",");
            pw.println("  \"mz_header_bytes\": " + IMAGE_BASE_FILE_OFF + ",");
            pw.println("  \"loaded_bytes\": " + loadedBytes + ",");
            pw.println("  \"bytes_in_identified_functions\": " + funcBytes + ",");
            pw.println("  \"bytes_in_game_functions\": " + gameFuncBytes + ",");
            pw.println("  \"bytes_disassembled_as_instructions\": " + insnBytes + ",");
            pw.println("  \"instruction_count\": " + insnCount + ",");
            pw.println("  \"instruction_bytes_outside_any_function\": "
                    + insnBytesOutsideFunctions + ",");
            pw.println("  \"bytes_defined_as_data\": " + dataBytes + ",");
            pw.println("  \"bytes_undefined\": " + undefinedBytes + ",");
            pw.println("  \"bytes_in_code_blocks\": " + codeBlockBytes + ",");
            pw.println("  \"bytes_in_code_blocks_inside_functions\": "
                    + codeBlockFuncBytes + ",");
            pw.println("  \"code_block_function_coverage_pct\": "
                    + String.format("%.1f", 100.0 * codeBlockFuncBytes / codeBlockBytes) + ",");
            pw.println("  \"unresolved_indirect_jumps\": " + indirectJumps + ",");
            pw.println("  \"unresolved_indirect_calls\": " + indirectCalls + ",");
            pw.println("  \"software_interrupts\": "
                    + (indirectJson.size() - indirectJumps - indirectCalls) + ",");
            pw.println("  \"branch_file_offset_mismatches\": " + fileOffMismatches + ",");
            pw.println("  \"conditional_branches\": " + totalBranches + ",");
            pw.println("  \"guards_resolved\": " + guardResolved + ",");
            pw.println("  \"guards_unresolved\": " + guardNull + ",");
            pw.println("  \"guards_resolved_across_a_join\": " + guardJoinCrossed + ",");
            pw.println("  \"caveat\": \"This is a LOWER BOUND. Branches inside bytes Ghidra did"
                    + " not disassemble are absent and cannot be counted from here.\"");
            pw.println("},");

            pw.println("\"memory_blocks\": [");
            pw.println(String.join(",\n", blockJson));
            pw.println("],");

            pw.println("\"undefined_runs\": [");
            pw.println(String.join(",\n", gapJson));
            pw.println("],");

            pw.println("\"port_citation_sources\": [\"src/**/*.rs\","
                    + " \"data/command_dispatch.json\"],");

            pw.println("\"functions\": [");
            pw.println(String.join(",\n", funcJson));
            pw.println("],");

            pw.println("\"uncited_spans\": [");
            pw.println(String.join(",\n", spanJson));
            pw.println("],");

            pw.println("\"unresolved_indirect_control_flow_detail\": [");
            pw.println(String.join(",\n", indirectJson));
            pw.println("],");

            pw.println("\"branches\": [");
            pw.println(String.join(",\n", branchJson));
            pw.println("]");
            pw.println("}");
        }

        println("BRANCHES branches=" + totalBranches
                + " functions=" + funcs.size()
                + " guards_null=" + guardNull
                + " indirect=" + indirectJson.size()
                + " -> " + jf);
    }

    // ------------------------------------------------------------------ guard

    private static class Guard {
        Instruction insn;
        int distance;
        String kind = "none";
        String status;
        boolean joinCrossed;
        boolean sharedWithPrecedingBranch;
        String flagSourceCall;
        Set<String> wanted = new LinkedHashSet<>();
    }

    /**
     * Find the instruction whose result the branch consumes: the nearest earlier
     * instruction in the same function that writes a register the branch reads.
     * Reports how far back it was and whether the walk crossed a jump target
     * (in which case the guard is only the guard on the fall-through path).
     */
    private Guard resolveGuard(Instruction branch, AddressSetView body) {
        Guard g = new Guard();
        for (Object o : branch.getInputObjects()) {
            if (o instanceof Register) {
                g.wanted.add(((Register) o).getName());
            }
        }
        Set<String> flagInputs = g.wanted.stream().filter(FLAG_REGS::contains)
                .collect(Collectors.toCollection(LinkedHashSet::new));
        Set<String> target = flagInputs.isEmpty() ? g.wanted : flagInputs;
        String kind = flagInputs.isEmpty() ? "counter" : "flags";

        if (target.isEmpty()) {
            g.status = "no_input_registers";
            return g;
        }

        Instruction cur = branch;
        for (int d = 1; d <= GUARD_SCAN_LIMIT; d++) {
            Instruction prev = listing.getInstructionBefore(cur.getAddress());
            if (prev == null || !body.contains(prev.getAddress())) {
                g.status = "reached_function_start";
                return g;
            }
            // Did we just step over a point other control flow jumps to?
            if (isJumpTarget(cur.getAddress())) {
                g.joinCrossed = true;
            }
            FlowType pft = prev.getFlowType();
            if (pft != null && (pft.isCall() || pft.isTerminal())) {
                g.status = "blocked_by_" + (pft.isCall() ? "call" : "return");
                if (pft.isCall()) {
                    // The flags are set inside the callee. Name it rather than
                    // guessing a guard: for this binary it is nearly always a
                    // Borland RTL compare helper.
                    g.flagSourceCall = prev.getFlows().length > 0
                            ? addr(prev.getFlows()[0]) : prev.toString();
                }
                return g;
            }
            if (pft != null && pft.isJump()) {
                if (pft.isConditional()) {
                    // A conditional jump does not write flags: the 32-bit compare
                    // idiom (cmp dx,bx / jg / jl / cmp ax,cx) shares one guard
                    // across several branches. Keep walking, but say so.
                    g.sharedWithPrecedingBranch = true;
                    cur = prev;
                    continue;
                }
                g.status = "blocked_by_jump";
                return g;
            }
            for (Object o : prev.getResultObjects()) {
                if (o instanceof Register && target.contains(((Register) o).getName())) {
                    g.insn = prev;
                    g.distance = d;
                    g.kind = kind;
                    g.status = g.joinCrossed ? "resolved_across_join" : "resolved";
                    return g;
                }
            }
            cur = prev;
        }
        g.status = "scan_limit_exceeded";
        return g;
    }

    /** Bytes from `a` to the closest port citation in the same function, or -1. */
    private long nearestCitation(Address a, Function f, Map<Address, List<String>> citations) {
        long best = -1;
        for (Address c : f.getBody().getAddresses(true)) {
            if (!citations.containsKey(c)) {
                continue;
            }
            long dist = Math.abs(c.getOffset() - a.getOffset());
            if (best < 0 || dist < best) {
                best = dist;
            }
        }
        return best;
    }

    private boolean isJumpTarget(Address a) {
        for (Reference r : rm.getReferencesTo(a)) {
            RefType t = r.getReferenceType();
            if (t.isJump()) {
                return true;
            }
        }
        return false;
    }

    // --------------------------------------------------------- classification

    private String classify(Function f) {
        return segOf(f.getEntryPoint()) == GHIDRA_BASE_SEGMENT ? "game" : "rtl";
    }

    // ------------------------------------------------------------- port scan

    /** address -> list of "file:line" citations of that address in the port. */
    private Map<Address, List<String>> scanPortCitations(String repoRoot) throws Exception {
        Map<Address, List<String>> out = new HashMap<>();
        List<Path> files = new ArrayList<>();
        Path src = Paths.get(repoRoot, "src");
        if (Files.isDirectory(src)) {
            try (Stream<Path> s = Files.walk(src)) {
                files.addAll(s.filter(p -> p.toString().endsWith(".rs"))
                        .sorted().collect(Collectors.toList()));
            }
        }
        Path cd = Paths.get(repoRoot, "data", "command_dispatch.json");
        if (Files.isRegularFile(cd)) {
            files.add(cd);
        }
        for (Path p : files) {
            List<String> lines;
            try {
                lines = Files.readAllLines(p, StandardCharsets.UTF_8);
            } catch (Exception e) {
                continue;
            }
            String rel = Paths.get(repoRoot).toAbsolutePath().relativize(p.toAbsolutePath())
                    .toString();
            for (int i = 0; i < lines.size(); i++) {
                Matcher m = CITATION.matcher(lines.get(i));
                while (m.find()) {
                    int seg = Integer.parseInt(m.group(1), 16);
                    int off = Integer.parseInt(m.group(2), 16);
                    // Docs cite real DOS segments (0f78:114b) and Ghidra segments
                    // (1000:af68) interchangeably. Normalise to Ghidra's.
                    int gseg = seg < GHIDRA_BASE_SEGMENT ? seg + GHIDRA_BASE_SEGMENT : seg;
                    Address a = addressOf(gseg, off);
                    if (a == null) {
                        continue;
                    }
                    out.computeIfAbsent(a, k -> new ArrayList<>()).add(rel + ":" + (i + 1));
                }
            }
        }
        return out;
    }

    private Address addressOf(int seg, int off) {
        try {
            Address a = currentProgram.getAddressFactory().getAddress(
                    String.format("%04x:%04x", seg, off));
            return (a != null && memory.contains(a)) ? a : null;
        } catch (Exception e) {
            return null;
        }
    }

    // -------------------------------------------------------------- indirect

    private String indirectRecord(Instruction ins, Function f, String fclass, String cond,
            String kind) {
        return "  {\"addr\": " + q(addr(ins.getAddress()))
                + ", \"file_off\": " + fileOffCalc(ins.getAddress())
                + ", \"bytes\": " + q(hex(ins))
                + ", \"func\": " + q(f == null ? null : f.getName())
                + ", \"class\": " + q(fclass)
                + ", \"conditional\": " + q(cond)
                + ", \"kind\": " + q(kind)
                + ", \"mnemonic\": " + q(ins.getMnemonicString())
                + ", \"text\": " + q(ins.toString())
                + ", \"resolved_targets\": " + ins.getFlows().length + "}";
    }

    // --------------------------------------------------------------- helpers

    private int segOf(Address a) {
        if (a instanceof SegmentedAddress) {
            return ((SegmentedAddress) a).getSegment();
        }
        return -1;
    }

    /** Offset WITHIN the segment. Address.getOffset() is the flat offset, not this. */
    private long offOf(Address a) {
        if (a instanceof SegmentedAddress) {
            return ((SegmentedAddress) a).getSegmentOffset();
        }
        return a.getOffset();
    }

    private String addr(Address a) {
        return a.toString();
    }

    private String realSegOff(Address a) {
        return String.format("%04x:%04x", segOf(a) - GHIDRA_BASE_SEGMENT, offOf(a));
    }

    private long fileOffCalc(Address a) {
        return IMAGE_BASE_FILE_OFF + (long) (segOf(a) - GHIDRA_BASE_SEGMENT) * 16 + offOf(a);
    }

    private String fileOffGhidra(Address a) {
        try {
            return Long.toString(memory.getAddressSourceInfo(a).getFileOffset());
        } catch (Exception e) {
            return "null";
        }
    }

    private String hex(Instruction ins) {
        StringBuilder sb = new StringBuilder();
        try {
            for (byte b : ins.getBytes()) {
                sb.append(String.format("%02x", b));
            }
        } catch (Exception e) {
            return "";
        }
        return sb.toString();
    }

    private String q(String s) {
        if (s == null) {
            return "null";
        }
        StringBuilder sb = new StringBuilder("\"");
        for (char c : s.toCharArray()) {
            switch (c) {
                case '"': sb.append("\\\""); break;
                case '\\': sb.append("\\\\"); break;
                case '\n': sb.append("\\n"); break;
                case '\r': sb.append("\\r"); break;
                case '\t': sb.append("\\t"); break;
                default:
                    if (c < 0x20) {
                        sb.append(String.format("\\u%04x", (int) c));
                    } else {
                        sb.append(c);
                    }
            }
        }
        return sb.append('"').toString();
    }

    private String qJoin(List<String> xs) {
        List<String> r = new ArrayList<>();
        for (String x : xs) {
            r.add(q(x));
        }
        return String.join(", ", r);
    }
}
