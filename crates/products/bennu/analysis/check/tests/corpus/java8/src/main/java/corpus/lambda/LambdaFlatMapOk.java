package corpus.lambda;

import java.util.Optional;
import java.util.stream.Stream;

/** The legal shapes of what {@link LambdaFlatMapBad} gets wrong: the function wraps its result, or `map` is used. */
public class LambdaFlatMapOk {
    static class Repo {
        Optional<String> codeOf(String key) {
            return Optional.of(key);
        }

        String plainCodeOf(String key) {
            return key;
        }
    }

    private final Repo repo = new Repo();

    Optional<String> wrapped(String key) {
        return Optional.ofNullable(key).flatMap(it -> repo.codeOf(it));
    }

    Optional<String> wrappedInline(String key) {
        return Optional.ofNullable(key).flatMap(it -> Optional.of(it + "!"));
    }

    Optional<String> blockLambda(String key) {
        return Optional.ofNullable(key).flatMap(it -> {
            if (it.isEmpty()) {
                return Optional.empty();
            }
            return Optional.of(it);
        });
    }

    Optional<String> nothing(String key) {
        return Optional.ofNullable(key).flatMap(it -> null);
    }

    Optional<String> methodReference(String key) {
        return Optional.ofNullable(key).flatMap(repo::codeOf);
    }

    Optional<String> mapForPlainValues(String key) {
        return Optional.ofNullable(key).map(it -> repo.plainCodeOf(it));
    }

    Stream<Character> streamOfStreams(Stream<String> texts) {
        return texts.flatMap(it -> it.chars().mapToObj(c -> (char) c));
    }

    Optional<Optional<String>> nested(String key) {
        return Optional.of(Optional.ofNullable(key)).map(inner -> inner.flatMap(it -> Optional.of(it)));
    }
}
