package corpus.statics.client;

import static corpus.statics.holder.Limits.clamp;
import static java.nio.charset.StandardCharsets.UTF_8;
import static java.util.Arrays.asList;
import static java.util.Collections.emptyList;
import static java.util.Collections.unmodifiableList;
import static java.util.Comparator.comparing;
import static java.util.Objects.requireNonNull;
import static java.util.concurrent.TimeUnit.SECONDS;
import static java.util.stream.Collectors.groupingBy;
import static java.util.stream.Collectors.joining;
import static java.util.stream.Collectors.toList;

import corpus.statics.holder.Level;

import java.util.List;
import java.util.Map;

/**
 * Statically imported JDK members whose type arguments are inferred from where they are used: a
 * generic factory in a return position, a collector in a stream pipeline, a comparator built from a
 * method reference, an enum constant of an imported class.
 */
public class StaticInference {
    public List<String> sortedByLength(List<String> input) {
        return input.stream().sorted(comparing(String::length)).collect(toList());
    }

    public Map<Integer, List<String>> byLength(List<String> words) {
        return words.stream().collect(groupingBy(String::length));
    }

    public String joined(List<Level> levels) {
        return levels.stream().map(Level::name).collect(joining(","));
    }

    public List<String> fixedNames() {
        return unmodifiableList(asList("a", "b"));
    }

    /** The target type, not the arguments, decides what {@code emptyList()} is a list of. */
    public List<Level> none() {
        return emptyList();
    }

    public long millis(long amount) {
        return SECONDS.toMillis(amount);
    }

    public byte[] bytes(String text) {
        return requireNonNull(text).getBytes(UTF_8);
    }

    public int clampedSum(List<Integer> values) {
        return values.stream().mapToInt(value -> clamp(value) * 2).sum();
    }
}
