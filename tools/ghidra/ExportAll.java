import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.ClangLine;
import ghidra.app.decompiler.ClangToken;
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.app.decompiler.DecompiledFunction;
import ghidra.app.decompiler.PrettyPrinter;
import ghidra.program.model.address.Address;
import ghidra.program.model.address.AddressIterator;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.FunctionManager;
import ghidra.program.model.symbol.IllegalCharCppTransformer;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceManager;

import java.io.File;
import java.io.PrintWriter;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.TreeSet;

/**
 * Exports one JSON record per function to {outDir}/functions.json, plus the
 * ADDRESS-ANNOTATED decompiled C of each function to {outDir}/decomp/.
 *
 * Two modes, selected by the second script argument:
 *
 *   full         (default) -- writes decomp/ AND functions.json.
 *   decomp-only            -- writes decomp/ ONLY.
 *
 * `decomp-only` exists because data/functions.json, data/branches.json and
 * data/string_pointers.json are COMMITTED and the port's coverage baseline is
 * measured against them.  Re-running the decompiler to refresh a build
 * artifact must not be able to move that baseline as a side effect.  See
 * run_ghidra.sh --decomp-only.
 *
 * EVERY collection serialised here is sorted before it is written. Ghidra hands
 * back Sets and reference iterators whose order is not stable between runs, so
 * an unsorted export made run_ghidra.sh rewrite data/functions.json with
 * reordered `calls` arrays on every run -- churn that hides a real change.
 * The output is a build artifact that is committed, so it has to be a function
 * of the program alone.
 *
 * `data_xrefs` carries the data references Ghidra already knows about, so that
 * `20ae:`-shaped questions ("what reads this byte?") can be answered from the
 * export instead of by scanning the image for operand bytes -- a scan that
 * cannot tell an operand from two adjacent instructions that happen to spell
 * the same word. See tools/re_query.py, subcommand `xrefs-to`.
 */
public class ExportAll extends GhidraScript {

    /** Marker opening the per-line address annotation. Kept in sync with
     *  the ANNOTATION regex in tools/decomp_addresses.py, which parses exactly
     *  this token and is asserted by tools/test_decomp_addresses.py. */
    private static final String ANNOT = "// @ ";

    /** Column the annotation starts at, when the code is shorter than this. */
    private static final int ANNOT_COL = 66;

    private String safe(String s) {
        return s.replaceAll("[^A-Za-z0-9_.-]", "_");
    }

    /**
     * The SET of machine addresses whose p-code contributed to one decompiled
     * line, as `SEG:OFF` strings, deduplicated and in address order.
     *
     * Deliberately NOT the line's minimum address. A decompiled line merges
     * several instructions -- an `if` line carries the compare, the branch and
     * often the operand loads -- and emitting one confident-looking address for
     * it is the "check that cannot fail, presented as verification" that
     * docs/re/METHODOLOGY.md names. Every token on the line is asked for its
     * own address, and both ends of each token's range are taken, so a token
     * that ever spans more than one instruction contributes both.
     *
     * ClangToken.getMinAddress() returns the token's p-code op's sequence
     * target -- i.e. the machine instruction that op came from -- or null for
     * a token with no op behind it (syntax, whitespace, type names). Nulls are
     * dropped, not guessed at.
     */
    private TreeSet<Address> lineAddresses(ClangLine line) {
        TreeSet<Address> addrs = new TreeSet<>();
        for (ClangToken t : line.getAllTokens()) {
            Address a = t.getMinAddress();
            if (a != null) addrs.add(a);
            Address b = t.getMaxAddress();
            if (b != null) addrs.add(b);
        }
        return addrs;
    }

    @Override
    public void run() throws Exception {
        String outDir = getScriptArgs().length > 0 ? getScriptArgs()[0] : "build";
        String mode = getScriptArgs().length > 1 ? getScriptArgs()[1] : "full";
        if (!mode.equals("full") && !mode.equals("decomp-only")) {
            throw new IllegalArgumentException(
                    "ExportAll: mode must be 'full' or 'decomp-only', got: " + mode);
        }
        boolean decompOnly = mode.equals("decomp-only");
        File decompDir = new File(outDir, "decomp");
        decompDir.mkdirs();

        DecompInterface di = new DecompInterface();
        di.openProgram(currentProgram);

        FunctionManager fm = currentProgram.getFunctionManager();
        ReferenceManager rm = currentProgram.getReferenceManager();

        List<String> json = new ArrayList<>();
        int decompiled = 0;

        for (Function f : fm.getFunctions(true)) {
            String name = f.getName();
            Address entry = f.getEntryPoint();

            DecompileResults res = di.decompileFunction(f, 60, monitor);

            File out = new File(decompDir, safe(name) + "_" + entry.toString().replace(":", "_") + ".c");
            try (PrintWriter pw = new PrintWriter(out, "UTF-8")) {
                if (!res.decompileCompleted()) {
                    pw.print("// DECOMPILATION FAILED: " + res.getErrorMessage() + "\n");
                } else {
                    writeAnnotated(pw, f, res);
                }
            }
            decompiled++;

            if (decompOnly) continue;

            // TreeSet: deduplicated AND ordered, so the serialisation is a
            // function of the program and not of Ghidra's iteration order.
            TreeSet<String> callers = new TreeSet<>();
            for (Reference r : rm.getReferencesTo(entry)) {
                Function cf = fm.getFunctionContaining(r.getFromAddress());
                if (cf != null) callers.add(cf.getName());
            }

            TreeSet<String> callees = new TreeSet<>();
            for (Function cf : f.getCalledFunctions(monitor)) {
                callees.add(cf.getName());
            }

            // Data references OUT of this function's body, to real memory
            // addresses. Stack references (Ghidra's "Stack[-0x102]" space) are
            // dropped: they are frame slots, not data addresses, and they
            // outnumber the real ones by more than ten to one.
            //
            // These are Ghidra's CLAIMS, not verified facts. Ghidra's DS/CS
            // tracking on this image is imperfect -- e.g. it records the
            // `mul [cs:0x11de]` at 1f78:11b1 as a reference to 20ae:06fe --
            // so tools/re_query.py re-checks every entry it uses against the
            // decoded operand at "at" before believing it.
            //
            // Sorted by the JSON text, which begins with the "at" address, so
            // the order is a function of the program and not of the iterator.
            List<String> xrefs = new ArrayList<>();
            AddressIterator ai = rm.getReferenceSourceIterator(f.getBody(), true);
            while (ai.hasNext()) {
                Address from = ai.next();
                for (Reference r : rm.getReferencesFrom(from)) {
                    if (!r.getReferenceType().isData()) continue;
                    if (!r.getToAddress().isMemoryAddress()) continue;
                    xrefs.add("{\"at\": \"" + from
                            + "\", \"to\": \"" + r.getToAddress()
                            + "\", \"type\": \"" + r.getReferenceType().getName()
                            + "\", \"op\": " + r.getOperandIndex() + "}");
                }
            }
            Collections.sort(xrefs);

            StringBuilder sb = new StringBuilder();
            sb.append("  {\"name\": \"").append(name).append("\"");
            sb.append(", \"entry\": \"").append(entry).append("\"");
            sb.append(", \"size\": ").append(f.getBody().getNumAddresses());
            sb.append(", \"called_by\": [").append(quoteJoin(new ArrayList<>(callers))).append("]");
            sb.append(", \"calls\": [").append(quoteJoin(new ArrayList<>(callees))).append("]");
            sb.append(", \"data_xrefs\": [").append(String.join(", ", xrefs)).append("]}");
            json.add(sb.toString());
        }

        if (!decompOnly) {
            File jf = new File(outDir, "functions.json");
            try (PrintWriter pw = new PrintWriter(jf, "UTF-8")) {
                pw.println("[");
                pw.println(String.join(",\n", json));
                pw.println("]");
            }
        }

        println("EXPORTED mode=" + mode + " decomp=" + decompiled
                + " functions_json=" + (decompOnly ? "SKIPPED" : String.valueOf(json.size()))
                + " to " + outDir);
        di.dispose();
    }

    /**
     * Write one function's decompiled C with a per-line address annotation.
     *
     * The C text is PrettyPrinter's own output -- the same bytes
     * DecompileResults.getDecompiledFunction().getC() produces, since that
     * method is exactly `new PrettyPrinter(...).print()`. Taking the lines from
     * the same PrettyPrinter instance that produced the text is what makes the
     * line <-> address correspondence structural rather than assumed: print()
     * emits one text line per ClangLine, in order.
     *
     * The count is still checked. If print() and getLines() ever disagree the
     * file gets an explicit ANNOTATION-DESYNC marker instead of silently
     * shifted addresses -- a wrong address that reads as authoritative is the
     * worst outcome here, worse than no annotation at all.
     */
    private void writeAnnotated(PrintWriter pw, Function f, DecompileResults res) {
        PrettyPrinter pp = new PrettyPrinter(f, res.getCCodeMarkup(), new IllegalCharCppTransformer());
        DecompiledFunction df = pp.print();
        List<ClangLine> lines = pp.getLines();

        // print() appends a line separator after EVERY line, so a split with a
        // negative limit yields one trailing empty element.
        String[] text = df.getC().split("\n", -1);
        int textCount = text.length;
        if (textCount > 0 && text[textCount - 1].isEmpty()) textCount--;

        // The header deliberately does NOT spell the annotation marker out.
        // The first draft did, and tools/test_decomp_addresses.py then read
        // nine English words per file as malformed addresses -- 1107 of them.
        // A legend that the legend's own parser mistakes for data is the
        // "inventory whose completeness claim stopped the next search" defect
        // in miniature, so the marker is described here, never quoted.
        pw.println("// Address annotation: a line that machine code contributed to ends in a C++");
        pw.println("// comment, a space, an at-sign, a space, then EVERY contributing address in");
        pw.println("// Ghidra segmented form -- not the line's minimum address, which would name");
        pw.println("// one instruction for a line that merges several.");
        pw.println("// Form A: file_off = 0x18d0 + (SEG - 0x1000) * 16 + OFF.");
        pw.println("// This is a LEAD, not evidence: verify each citation with");
        pw.println("// `python3 tools/re_query.py resolve <citation>` before writing it down.");

        if (textCount != lines.size()) {
            pw.println("// ANNOTATION-DESYNC: PrettyPrinter emitted " + textCount
                    + " text lines but " + lines.size()
                    + " ClangLines; addresses withheld rather than misaligned.");
            pw.print(df.getC());
            return;
        }

        for (int i = 0; i < textCount; i++) {
            String code = text[i];
            TreeSet<Address> addrs = lineAddresses(lines.get(i));
            if (addrs.isEmpty()) {
                pw.println(code);
                continue;
            }
            StringBuilder sb = new StringBuilder(code);
            while (sb.length() < ANNOT_COL) sb.append(' ');
            sb.append("  ").append(ANNOT);
            boolean first = true;
            for (Address a : addrs) {
                if (!first) sb.append(' ');
                sb.append(a.toString());
                first = false;
            }
            pw.println(sb.toString());
        }
    }

    private String quoteJoin(List<String> xs) {
        List<String> q = new ArrayList<>();
        for (String x : xs) q.add("\"" + x + "\"");
        return String.join(", ", q);
    }
}
