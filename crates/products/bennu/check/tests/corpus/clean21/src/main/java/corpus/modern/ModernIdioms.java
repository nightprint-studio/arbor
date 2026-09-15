package corpus.modern;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.StringReader;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Comparator;
import java.util.Iterator;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Optional;
import java.util.TreeMap;
import java.util.function.BiFunction;
import java.util.function.Function;
import java.util.function.Predicate;
import java.util.function.Supplier;
import java.util.stream.Collectors;

/**
 * {@code var} (also on lambda parameters), text blocks, private interface methods, an effectively
 * final try-with-resources resource, anonymous classes with diamond, {@code Stream.toList()},
 * {@code Collectors.teeing}, {@code Optional.or}.
 */
public final class ModernIdioms {

    private ModernIdioms() {
    }

    public interface Formatter {
        String format(String value);

        default String formatAll(List<String> values) {
            return values.stream().map(this::guarded).collect(Collectors.joining(separator()));
        }

        private String guarded(String value) {
            return value == null ? "" : format(value);
        }

        private static String separator() {
            return ", ";
        }
    }

    public static String report(List<String> names) {
        var header = """
                Report
                ======
                """;
        var body = new StringBuilder(header);
        for (var name : names) {
            body.append("- %s%n".formatted(name));
        }
        return body.toString();
    }

    public static String json(String name, int age) {
        return """
                {
                  "name": "%s",
                  "age": %d
                }\
                """.formatted(name, age);
    }

    public static List<String> shout(List<String> values) {
        BiFunction<String, Integer, String> repeat = (var text, var times) -> text.repeat(times);
        Function<String, String> upper = (final var text) -> text.toUpperCase(Locale.ROOT);
        return values.stream().map(upper).map(text -> repeat.apply(text, 2)).toList();
    }

    public static String firstLine(String content) throws IOException {
        var reader = new BufferedReader(new StringReader(content));
        try (reader) {
            var line = reader.readLine();
            return line == null ? "" : line;
        }
    }

    public static Comparator<String> byLengthThenAlpha() {
        return new Comparator<>() {
            @Override
            public int compare(String left, String right) {
                var difference = Integer.compare(left.length(), right.length());
                return difference != 0 ? difference : left.compareTo(right);
            }
        };
    }

    public static <T> Iterator<T> cycle(List<T> items) {
        return new Iterator<>() {
            private int index;

            @Override
            public boolean hasNext() {
                return !items.isEmpty();
            }

            @Override
            public T next() {
                var value = items.get(index);
                index = (index + 1) % items.size();
                return value;
            }
        };
    }

    public static Map<String, List<Integer>> grouped() {
        var map = new TreeMap<String, List<Integer>>(Map.of("a", List.of(1, 2), "b", List.of(3)));
        map.computeIfAbsent("c", key -> new ArrayList<>()).add(4);
        return Collections.unmodifiableMap(map);
    }

    public static String stats(List<Integer> values) {
        record MinMax(int min, int max) {
        }
        var result = values.stream().collect(Collectors.teeing(
                Collectors.minBy(Comparator.<Integer>naturalOrder()),
                Collectors.maxBy(Comparator.<Integer>naturalOrder()),
                (min, max) -> new MinMax(min.orElse(0), max.orElse(0))));
        return result.min() + ".." + result.max();
    }

    public static String pick(Optional<String> primary, Supplier<Optional<String>> fallback) {
        return primary.or(fallback).filter(Predicate.not(String::isBlank)).orElse("none");
    }

    public static List<String> sortedCopy(List<String> values) {
        var copy = new ArrayList<>(values);
        copy.sort(byLengthThenAlpha());
        return List.copyOf(copy);
    }

    public static String demo() throws IOException {
        Formatter brackets = value -> "[" + value + "]";
        var names = List.of("ada", "linus", "grace");
        var looped = cycle(names);
        return brackets.formatAll(names) + report(names) + json("ada", 36) + shout(names)
                + firstLine("one\ntwo") + sortedCopy(names) + looped.next() + grouped()
                + stats(List.of(4, 1, 9)) + pick(Optional.empty(), () -> Optional.of("fallback"));
    }
}
