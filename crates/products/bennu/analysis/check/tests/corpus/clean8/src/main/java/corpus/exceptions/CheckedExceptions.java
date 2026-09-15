package corpus.exceptions;

import java.io.BufferedReader;
import java.io.FileNotFoundException;
import java.io.IOException;
import java.io.StringReader;
import java.io.UncheckedIOException;
import java.util.ArrayList;
import java.util.List;
import java.util.Properties;
import java.util.concurrent.Callable;
import java.util.function.Function;
import java.util.stream.Collectors;

/**
 * Checked exceptions handled correctly: declared, caught, wrapped, narrowed by overrides, and carried
 * through functional interfaces whose method declares {@code throws}.
 */
public class CheckedExceptions {

    public static class ConfigException extends Exception {
        private static final long serialVersionUID = 1L;

        public ConfigException(String message, Throwable cause) {
            super(message, cause);
        }
    }

    @FunctionalInterface
    public interface IoFunction<T, R> {
        R apply(T input) throws IOException;
    }

    @FunctionalInterface
    public interface Task<V, X extends Exception> {
        V call() throws X;
    }

    public static <T, R> Function<T, R> unchecked(final IoFunction<T, R> function) {
        return new Function<T, R>() {
            @Override
            public R apply(T input) {
                try {
                    return function.apply(input);
                } catch (IOException e) {
                    throw new UncheckedIOException(e);
                }
            }
        };
    }

    static String load(String name) throws IOException {
        if (name.isEmpty()) {
            throw new FileNotFoundException("empty name");
        }
        return "mode=" + name;
    }

    public static List<String> loadAll(List<String> names) {
        return names.stream().map(unchecked(CheckedExceptions::load)).collect(Collectors.toList());
    }

    public static Properties readConfig(String name) throws ConfigException {
        try {
            return parse(load(name));
        } catch (IOException e) {
            throw new ConfigException("cannot read " + name, e);
        }
    }

    private static Properties parse(String text) throws IOException {
        Properties properties = new Properties();
        properties.load(new StringReader(text));
        return properties;
    }

    public static <V, X extends Exception> V firstNonNull(Task<V, X> primary, Task<V, X> fallback) throws X {
        V value = primary.call();
        return value != null ? value : fallback.call();
    }

    /** {@code X} is inferred as ConfigException from what the lambda bodies throw. */
    public static String mode() throws ConfigException {
        return firstNonNull(
                () -> readConfig("primary").getProperty("mode"),
                () -> readConfig("fallback").getProperty("mode", "safe"));
    }

    public static int countLines(final String text) {
        Callable<Integer> counter = () -> {
            BufferedReader reader = new BufferedReader(new StringReader(text));
            int lines = 0;
            while (reader.readLine() != null) {
                lines++;
            }
            return lines;
        };
        try {
            return counter.call();
        } catch (Exception e) {
            return -1;
        }
    }

    public static String rootCause(Throwable error) {
        Throwable current = error;
        while (current.getCause() != null && current.getCause() != current) {
            current = current.getCause();
        }
        return current.getClass().getName();
    }

    public abstract static class Loader {
        public abstract String fetch(String key) throws IOException, ConfigException;
    }

    /** An override may declare fewer, narrower checked exceptions. */
    public static final class CachedLoader extends Loader {
        private final List<String> keys = new ArrayList<String>();

        @Override
        public String fetch(String key) throws FileNotFoundException {
            if (key.isEmpty()) {
                throw new FileNotFoundException("empty key");
            }
            keys.add(key);
            return key + keys.size();
        }
    }

    public static String useLoaders() throws ConfigException {
        CachedLoader cached = new CachedLoader();
        Loader general = cached;
        try {
            return cached.fetch("a") + general.fetch("b");
        } catch (IOException e) {
            return "failed: " + e.getMessage() + " " + rootCause(e);
        }
    }

    public static String demo() {
        try {
            return mode() + loadAll(java.util.Arrays.asList("x", "y")) + countLines("a\nb") + useLoaders();
        } catch (ConfigException e) {
            return rootCause(e);
        } catch (UncheckedIOException e) {
            return "unchecked " + e.getCause().getMessage();
        }
    }
}
