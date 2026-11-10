package corpus.lambda;

import java.util.Optional;
import java.util.stream.Stream;

/** `flatMap` with a function that returns a plain value instead of an `Optional` / `Stream` (`map` was meant). Twin: {@link LambdaFlatMapOk}. */
public class LambdaFlatMapBad {
    static class Repo {
        String codeOf(String key) {
            return key;
        }
    }

    private final Repo repo = new Repo();

    String plain(String text) {
        return text;
    }

    int number(String text) {
        return text.length();
    }

    Optional<String> expressionLambda(String key) {
        return Optional.ofNullable(key).flatMap(it -> plain(it)); // error: compiler.err.prob.found.req
    }

    Optional<Integer> primitiveResult(String key) {
        return Optional.ofNullable(key).flatMap(it -> number(it)); // error: compiler.err.prob.found.req
    }

    Optional<String> blockLambda(String key) {
        return Optional.ofNullable(key).flatMap(it -> { return plain(it); }); // error: compiler.err.prob.found.req
    }

    String fromARepository(String key) {
        return Optional.ofNullable(key).flatMap(it -> repo.codeOf(it)).orElse(null); // error: compiler.err.prob.found.req
    }

    Stream<Integer> streamFlatMap(Stream<String> texts) {
        return texts.flatMap(it -> number(it)); // error: compiler.err.prob.found.req
    }
}
