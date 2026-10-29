// Arbor/Bennu: reads a JDK Flight Recorder file and prints one compact JSON line per event of
// interest. Used only when the JDK has no `jfr` tool (JDK 11, JDK 8u262+).
// Java 8 syntax on purpose: it is compiled with the JDK 8 javac in one case.
import java.io.BufferedWriter;
import java.io.OutputStreamWriter;
import java.nio.charset.StandardCharsets;
import java.nio.file.Paths;
import jdk.jfr.consumer.RecordedClass;
import jdk.jfr.consumer.RecordedEvent;
import jdk.jfr.consumer.RecordedFrame;
import jdk.jfr.consumer.RecordedMethod;
import jdk.jfr.consumer.RecordedStackTrace;
import jdk.jfr.consumer.RecordedThread;
import jdk.jfr.consumer.RecordingFile;

public class ArborJfrDump {
    private static final int MAX_FRAMES = 128;

    public static void main(String[] args) throws Exception {
        BufferedWriter out = new BufferedWriter(new OutputStreamWriter(System.out, StandardCharsets.UTF_8), 1 << 16);
        try (RecordingFile file = new RecordingFile(Paths.get(args[0]))) {
            while (file.hasMoreEvents()) {
                RecordedEvent ev;
                try {
                    ev = file.readEvent();
                } catch (Throwable t) {
                    break; // truncated tail of a recording that was cut: keep what was read
                }
                String name = ev.getEventType().getName();
                switch (name) {
                    case "jdk.ExecutionSample":
                        line(out, "x", threadName(ev, "sampledThread"), null, 0, stack(ev), null, null);
                        break;
                    case "jdk.ObjectAllocationSample":
                        line(out, "a", threadName(ev, "eventThread"), className(ev, "objectClass"),
                                ev.hasField("weight") ? ev.getLong("weight") : 0, stack(ev), null, null);
                        break;
                    case "jdk.JavaMonitorEnter":
                        line(out, "l", threadName(ev, "eventThread"), className(ev, "monitorClass"),
                                ev.getDuration().toNanos(), stack(ev), null, null);
                        break;
                    case "jdk.GarbageCollection":
                        line(out, "g", null, str(ev, "name"), ev.getDuration("sumOfPauses").toNanos(), null,
                                str(ev, "cause"), ev.hasField("longestPause") ? Long.valueOf(ev.getDuration("longestPause").toNanos()) : null);
                        break;
                    default:
                        break;
                }
            }
        }
        out.flush();
    }

    private static String str(RecordedEvent ev, String f) {
        try { Object o = ev.getValue(f); return o == null ? "" : o.toString(); } catch (Throwable t) { return ""; }
    }

    private static String threadName(RecordedEvent ev, String f) {
        try {
            RecordedThread t = ev.getThread(f);
            if (t == null) return "";
            String n = t.getJavaName();
            return n != null ? n : String.valueOf(t.getOSName());
        } catch (Throwable t) { return ""; }
    }

    private static String className(RecordedEvent ev, String f) {
        try { RecordedClass c = ev.getClass(f); return c == null ? "" : c.getName(); } catch (Throwable t) { return ""; }
    }

    private static RecordedStackTrace stack(RecordedEvent ev) {
        try { return ev.getStackTrace(); } catch (Throwable t) { return null; }
    }

    private static void line(BufferedWriter o, String kind, String thread, String cls, long amount,
                             RecordedStackTrace st, String cause, Long longest) throws Exception {
        StringBuilder b = new StringBuilder(256);
        b.append("{\"e\":\"").append(kind).append('"');
        if (thread != null) { b.append(",\"t\":"); quote(b, thread); }
        if (cls != null) { b.append(",\"c\":"); quote(b, cls); }
        if (amount != 0) b.append(",\"n\":").append(amount);
        if (cause != null) { b.append(",\"cause\":"); quote(b, cause); }
        if (longest != null) b.append(",\"max\":").append(longest.longValue());
        if (st != null) {
            b.append(",\"f\":[");
            int i = 0;
            for (RecordedFrame f : st.getFrames()) {
                if (i >= MAX_FRAMES) break;
                RecordedMethod m = f.getMethod();
                if (i++ > 0) b.append(',');
                b.append('[');
                quote(b, m.getType().getName()); b.append(',');
                quote(b, m.getName()); b.append(',');
                quote(b, m.getDescriptor()); b.append(',');
                b.append(f.getLineNumber()).append(']');
            }
            b.append(']');
        }
        b.append("}\n");
        o.write(b.toString());
    }

    private static void quote(StringBuilder b, String s) {
        b.append('"');
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            if (c == '"' || c == '\') b.append('\').append(c);
            else if (c < 0x20) b.append(String.format("\u%04x", (int) c));
            else b.append(c);
        }
        b.append('"');
    }
}
