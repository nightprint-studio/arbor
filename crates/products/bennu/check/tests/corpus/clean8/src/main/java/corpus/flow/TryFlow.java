package corpus.flow;

import java.io.BufferedReader;
import java.io.Closeable;
import java.io.FileInputStream;
import java.io.FileNotFoundException;
import java.io.FileReader;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.io.Reader;
import java.io.StringReader;
import java.io.UnsupportedEncodingException;
import java.lang.reflect.InvocationTargetException;
import java.util.ArrayList;
import java.util.List;

/** try-with-resources, multi-catch, precise rethrow, nested try, finally. */
public class TryFlow {

    private final List<String> log = new ArrayList<String>();

    public List<String> log() {
        return log;
    }

    public String firstLine(String path) throws IOException {
        try (FileReader reader = new FileReader(path);
             BufferedReader buffered = new BufferedReader(reader)) {
            String line = buffered.readLine();
            return line == null ? "" : line;
        }
    }

    /** Two unrelated subclasses of IOException in a multi-catch, then IOException itself. */
    public String readAll(String path, String charset) {
        try {
            InputStream in = new FileInputStream(path);
            try {
                Reader reader = new InputStreamReader(in, charset);
                StringBuilder out = new StringBuilder();
                char[] buffer = new char[256];
                int read;
                while ((read = reader.read(buffer)) != -1) {
                    out.append(buffer, 0, read);
                }
                return out.toString();
            } finally {
                in.close();
            }
        } catch (FileNotFoundException | UnsupportedEncodingException e) {
            return "missing: " + e.getMessage();
        } catch (IOException e) {
            return "io: " + e.getMessage();
        }
    }

    /** Precise rethrow: {@code catch (Exception e) { throw e; }} only rethrows what the try can throw. */
    public Object instantiate(String className)
            throws ClassNotFoundException, NoSuchMethodException, InstantiationException,
            IllegalAccessException, InvocationTargetException {
        try {
            return Class.forName(className).getConstructor().newInstance();
        } catch (Exception e) {
            log.add("cannot instantiate " + className);
            throw e;
        }
    }

    public void closeQuietly(Closeable... resources) {
        for (Closeable resource : resources) {
            try {
                resource.close();
            } catch (IOException | RuntimeException e) {
                log.add("close failed: " + e);
            }
        }
    }

    static final class Lease implements AutoCloseable {
        private final String name;
        private final List<String> journal;

        Lease(String name, List<String> journal) throws IOException {
            if (name.isEmpty()) {
                throw new IOException("unnamed lease");
            }
            this.name = name;
            this.journal = journal;
            journal.add("open " + name);
        }

        /** Narrows {@code AutoCloseable.close() throws Exception} to no checked exception. */
        @Override
        public void close() {
            journal.add("close " + name);
        }
    }

    public int withLeases(String first, String second) {
        try (Lease a = new Lease(first, log); Lease b = new Lease(second, log)) {
            return a.name.length() + b.name.length();
        } catch (IOException e) {
            for (Throwable suppressed : e.getSuppressed()) {
                log.add("suppressed " + suppressed);
            }
            return -1;
        } finally {
            log.add("leases done");
        }
    }

    public int parseAll(String csv) {
        int total = 0;
        for (String part : csv.split(",")) {
            try {
                total += Integer.parseInt(part.trim());
            } catch (NumberFormatException e) {
                try {
                    total += (int) Double.parseDouble(part);
                } catch (NumberFormatException again) {
                    throw new IllegalArgumentException("not a number: " + part, again);
                }
            }
        }
        return total;
    }

    public List<String> lines(String text) {
        List<String> out = new ArrayList<String>();
        BufferedReader reader = new BufferedReader(new StringReader(text));
        try {
            String line;
            while ((line = reader.readLine()) != null) {
                out.add(line);
            }
        } catch (IOException e) {
            throw new IllegalStateException("a StringReader cannot fail", e);
        } finally {
            try {
                reader.close();
            } catch (IOException ignored) {
                log.add("close failed");
            }
        }
        return out;
    }
}
