package corpus.infer;

import java.util.Comparator;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.OptionalDouble;
import java.util.Set;
import java.util.function.BiFunction;
import java.util.function.Function;
import java.util.function.Supplier;
import java.util.stream.Collectors;
import java.util.stream.IntStream;
import java.util.stream.Stream;

/** Streams, collectors and functional interfaces — where the result type depends on a lambda or a method reference. */
public class InferStreams {
    void pipelines(List<String> names) {
        Stream<String> stream = names.stream(); // infer
        Stream<Integer> lengths = names.stream().map(String::length); // infer
        IntStream ints = names.stream().mapToInt(String::length); // infer
        int sum = names.stream().mapToInt(String::length).sum(); // infer
        Optional<String> first = names.stream().findFirst(); // infer
        long count = names.stream().count(); // infer
        boolean any = names.stream().anyMatch(s -> s.isEmpty()); // infer
        Stream<String> flat = names.stream().flatMap(s -> Stream.of(s, s)); // infer
        OptionalDouble average = IntStream.range(0, 3).average(); // infer
        List<String> viaToList = names.stream().toList(); // infer
    }

    void collectors(List<String> names) {
        List<String> trimmed = names.stream().map(String::trim).collect(Collectors.toList()); // infer
        List<String> filtered = names.stream().filter(s -> !s.isEmpty()).collect(Collectors.toList()); // infer
        Set<String> unique = names.stream().collect(Collectors.toSet()); // infer
        String joined = names.stream().collect(Collectors.joining(",")); // infer
        Map<Integer, List<String>> byLength = names.stream().collect(Collectors.groupingBy(String::length)); // infer
        Map<Boolean, List<String>> split = names.stream().collect(Collectors.partitioningBy(s -> s.isEmpty())); // infer
        Map<String, Integer> index = names.stream().collect(Collectors.toMap(s -> s, String::length)); // infer
        List<Integer> sorted = Stream.of(3, 1, 2).sorted().collect(Collectors.toList()); // infer
        Optional<Integer> largest = Stream.of(3, 1).max(Comparator.naturalOrder()); // infer
    }

    /** An array is the varargs of `Stream.of` / `List.of`, not one element: both give the element type. */
    void arrays(String[] array, Class<Thread.State> stateType) {
        Stream<String> fromArray = Stream.of(array); // infer
        Stream<String> fromElements = Stream.of("a", "b"); // infer
        Stream<String> single = Stream.of("a"); // infer
        List<String> listFromArray = List.of(array); // infer
        Thread.State[] constants = stateType.getEnumConstants(); // infer
    }

    void functional(List<String> names) {
        Function<String, Integer> length = String::length;
        Integer applied = length.apply("x"); // infer
        Supplier<List<String>> supplier = () -> names;
        List<String> supplied = supplier.get(); // infer
        BiFunction<Integer, Integer, Integer> add = (a, b) -> a + b;
        Integer total = add.apply(1, 2); // infer
        Comparator<String> byLength = Comparator.comparing(String::length); // infer
        Function<String, String> identity = Function.identity();
        Function<String, Integer> composed = identity.andThen(String::length); // infer
    }
}
