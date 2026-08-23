import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.program.model.address.Address;
import ghidra.program.model.address.AddressIterator;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.FunctionManager;
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
 * decompiled C of each function to {outDir}/decomp/.
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

    private String safe(String s) {
        return s.replaceAll("[^A-Za-z0-9_.-]", "_");
    }

    @Override
    public void run() throws Exception {
        String outDir = getScriptArgs().length > 0 ? getScriptArgs()[0] : "build";
        File decompDir = new File(outDir, "decomp");
        decompDir.mkdirs();

        DecompInterface di = new DecompInterface();
        di.openProgram(currentProgram);

        FunctionManager fm = currentProgram.getFunctionManager();
        ReferenceManager rm = currentProgram.getReferenceManager();

        List<String> json = new ArrayList<>();

        for (Function f : fm.getFunctions(true)) {
            String name = f.getName();
            Address entry = f.getEntryPoint();

            DecompileResults res = di.decompileFunction(f, 60, monitor);
            String c = res.decompileCompleted()
                    ? res.getDecompiledFunction().getC()
                    : "// DECOMPILATION FAILED: " + res.getErrorMessage() + "\n";

            File out = new File(decompDir, safe(name) + "_" + entry.toString().replace(":", "_") + ".c");
            try (PrintWriter pw = new PrintWriter(out, "UTF-8")) {
                pw.print(c);
            }

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

        File jf = new File(outDir, "functions.json");
        try (PrintWriter pw = new PrintWriter(jf, "UTF-8")) {
            pw.println("[");
            pw.println(String.join(",\n", json));
            pw.println("]");
        }

        println("EXPORTED functions=" + json.size() + " to " + outDir);
        di.dispose();
    }

    private String quoteJoin(List<String> xs) {
        List<String> q = new ArrayList<>();
        for (String x : xs) q.add("\"" + x + "\"");
        return String.join(", ", q);
    }
}
