package corpus.infer;

import corpus.infer.InferTypes.Box;
import corpus.infer.InferTypes.Names;
import corpus.infer.InferTypes.Pair;
import corpus.infer.InferTypes.Util;

import java.util.Iterator;
import java.util.List;

/** The project's own generic types and methods: substitution through a class, a supertype and a method's type variables. */
public class InferGenerics {
    void generic_class(List<String> names) {
        Box<String> box = new Box<>("x"); // infer
        String value = box.get(); // infer
        Box<Integer> mapped = box.map(String::length); // infer
        Box<List<String>> nested = new Box<List<String>>(names); // infer
        List<String> inner = nested.get(); // infer
        int nestedSize = nested.get().size(); // infer
    }

    void generic_methods(List<String> names) {
        Pair<String, Integer> pair = Pair.of("a", 1); // infer
        Integer second = pair.second(); // infer
        String first = pair.first(); // infer
        Pair<Integer, String> swapped = pair.swap(); // infer
        String largest = Util.max(names); // infer
        List<String> copy = Util.copyOf(names); // infer
    }

    void through_a_supertype(Names named) {
        String got = named.get(0); // infer
        Iterator<String> iterator = named.iterator(); // infer
        int size = named.size(); // infer
        List<String> asList = named.subList(0, 1); // infer
    }

    void wildcards(List<? extends Number> numbers, List<? super Integer> sink) {
        Number head = numbers.get(0); // infer
        int size = numbers.size(); // infer
        Object taken = sink.get(0); // infer?
    }
}
