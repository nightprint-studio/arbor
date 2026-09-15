package corpus.flow;

import java.util.Iterator;
import java.util.List;
import java.util.Queue;

/**
 * Definite assignment and reachability that javac accepts: finals assigned on every path, loops
 * that only exit through {@code break}, labels, methods that end in {@code throw} or never end.
 */
public final class DefiniteAssignment {

    private DefiniteAssignment() {
    }

    static String classify(int code) {
        final String label;
        if (code < 0) {
            label = "negative";
        } else if (code == 0) {
            label = "zero";
        } else {
            label = "positive";
        }
        return label;
    }

    static int weight(char grade) {
        final int weight;
        switch (grade) {
            case 'A':
                weight = 4;
                break;
            case 'B':
            case 'C':
                weight = 2;
                break;
            case 'F':
            default:
                weight = 0;
                break;
        }
        return weight;
    }

    static String mimeType(String extension) {
        final String type;
        switch (extension.toLowerCase()) {
            case "htm":
            case "html":
                type = "text/html";
                break;
            case "png":
                type = "image/png";
                break;
            default:
                type = "application/octet-stream";
        }
        return type;
    }

    /** Assigned in {@code try}; the only catch rethrows, so it is assigned after the statement. */
    static int parsePort(String text) {
        final int port;
        try {
            port = Integer.parseInt(text);
        } catch (NumberFormatException e) {
            throw new IllegalArgumentException("bad port " + text, e);
        }
        return port;
    }

    static long parseSize(String text) {
        final long size;
        try {
            size = Long.parseLong(text);
        } catch (NumberFormatException e) {
            return -1L;
        }
        return size;
    }

    static int lengthLogged(String path, List<String> log) {
        final int length;
        try {
            length = path.length();
        } finally {
            log.add("checked " + path);
        }
        return length;
    }

    /** Assigned only on the path that breaks out of {@code while (true)}. */
    static int firstPositive(int[] values) {
        int index = 0;
        int result;
        while (true) {
            if (values[index] > 0) {
                result = values[index];
                break;
            }
            index++;
        }
        return result;
    }

    static int readUntilPositive(Iterator<Integer> source) {
        int value;
        do {
            value = source.next();
        } while (value <= 0);
        return value;
    }

    /** Assigned inside the right operand of {@code &&}: definitely assigned when the condition is true. */
    static boolean hasLongWord(String text) {
        int length;
        if (text != null && (length = text.trim().length()) > 10) {
            return length > 20 || text.indexOf(' ') < 0;
        }
        return false;
    }

    /** Assigned once on each branch, never again: effectively final, so a lambda may capture it. */
    static Runnable greeter(boolean formal, final List<String> sink) {
        String greeting;
        if (formal) {
            greeting = "Good morning";
        } else {
            greeting = "Hi";
        }
        return () -> sink.add(greeting);
    }

    static String firstMatch(String[][] table, String needle) {
        String hit = null;
        rows:
        for (String[] row : table) {
            for (String cell : row) {
                if (cell.isEmpty()) {
                    continue rows;
                }
                if (cell.equals(needle)) {
                    hit = cell;
                    break rows;
                }
            }
        }
        return hit;
    }

    static int countPairs(int[] values, int target) {
        int pairs = 0;
        int i = 0;
        outer:
        while (i < values.length) {
            int j = i + 1;
            i++;
            while (j < values.length) {
                if (values[i - 1] + values[j] == target) {
                    pairs++;
                    continue outer;
                }
                j++;
            }
        }
        return pairs;
    }

    /** Ends in {@code throw}: no return statement needed. */
    static int unsupported(String operation) {
        throw new UnsupportedOperationException(operation);
    }

    /** The loop never completes normally, so the method needs no trailing return. */
    static int firstAbove(Iterator<Integer> source, int limit) {
        for (;;) {
            int next = source.next();
            if (next > limit) {
                return next;
            }
        }
    }

    static void drain(Queue<Runnable> jobs) {
        while (true) {
            Runnable job = jobs.poll();
            if (job == null) {
                return;
            }
            job.run();
        }
    }

    static int awaitForever(Object lock) throws InterruptedException {
        synchronized (lock) {
            while (true) {
                lock.wait();
            }
        }
    }

    static String sign(int value) {
        if (value > 0) {
            return "+";
        } else if (value < 0) {
            return "-";
        } else {
            return "0";
        }
    }
}
