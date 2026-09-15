package corpus.generic;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.Optional;

/** Legal twins of {@link GenericBad}, plus substitution and inference a naive checker gets wrong. */
public class GenericOk {
    void takesString(String value) {
    }

    void takesInt(int value) {
    }

    void takesObject(Object value) {
    }

    void takesStrings(List<String> values) {
    }

    class Node<T> {
        T value;
    }

    <T> void addTo(T value, List<T> into) {
        into.add(value);
    }

    void optionalOrElse(Optional<String> text) {
        text.orElse("fallback");
        text.orElse(null);
    }

    void listAdd(List<String> names) {
        names.add("x");
        names.add(0, "x");
    }

    void listGet(List<String> names) {
        names.get(0);
    }

    void mapPut(Map<String, Integer> counts) {
        counts.put("key", 1);
    }

    void boxSet(GenericApi.Box<String> box) {
        box.set("x");
    }

    void inheritedGenericMembers(GenericApi.StringBox box) {
        box.set("x");
        takesString(box.get());
    }

    void interfaceDefault(GenericApi.Holder<String> holder) {
        takesString(holder.orDefault("fallback"));
    }

    void interfaceImplementedByALambda() {
        GenericApi.Holder<String> holder = () -> "x";
        takesString(holder.raw());
    }

    void typeParameterBoundSatisfied() {
        GenericApi.num(1);
        GenericApi.num(1.5);
    }

    void wildcardBoundsSatisfied(List<Integer> integers, List<Number> numbers, List<Object> objects) {
        GenericApi.sum(integers);
        GenericApi.fill(numbers);
        GenericApi.fill(objects);
    }

    void invariantListExact(List<Number> numbers) {
        GenericApi.sumExact(numbers);
    }

    void recursiveBoundSatisfied() {
        GenericApi.maxOf("a", "b");
        GenericApi.maxOf(1, 2);
    }

    void inferredTypesAgree(Map<String, Integer> counts) {
        GenericApi.put(counts, "key", 1);
    }

    void genericReturnsAsArguments(GenericApi.Box<String> box, Optional<String> text, List<String> names) {
        takesString(box.get());
        takesString(text.get());
        takesString(GenericApi.first(names));
        takesInt(names.get(0).length());
    }

    void resultTypeInferredFromALambdaBody(GenericApi.Box<String> box) {
        takesString(box.map(value -> value + "!").get());
        int length = box.map(String::length).get();
    }

    void rawTypes() {
        List raw = new ArrayList();
        raw.add(1);
        takesObject(raw.get(0));
        takesString((String) raw.get(0));
    }

    void targetTypedArguments() {
        takesStrings(Collections.emptyList());
        takesStrings(new ArrayList<>());
        takesStrings(Arrays.asList("a"));
        takesStrings(Collections.<String>emptyList());
    }

    void wildcardReceivers(List<? extends Number> numbers, Map<String, List<Integer>> nested) {
        Number first = numbers.get(0);
        nested.get("key").add(1);
    }

    void iterableForEach(List<String> names) {
        Iterable<String> iterable = names;
        for (String name : iterable) {
            takesString(name);
        }
    }

    void genericMethodOfThisClass(List<String> names) {
        addTo("x", names);
    }

    void innerGenericClass() {
        Node<String> node = new Node<String>();
        takesString(node.value);
    }

    void sortWithNaturalOrder(List<String> names) {
        Collections.sort(names);
    }
}
