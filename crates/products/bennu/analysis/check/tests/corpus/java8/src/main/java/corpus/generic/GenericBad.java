package corpus.generic;

import java.util.List;
import java.util.Map;
import java.util.Optional;

/** Arguments that fail once type arguments are substituted or inferred. Twin: {@link GenericOk}. */
public class GenericBad {
    void takesString(String value) {
    }

    void optionalOrElseWrongType(Optional<String> text) {
        text.orElse(1); // error: compiler.err.cant.apply.symbol
    }

    void listAddWrongType(List<String> names) {
        names.add(1); // error: compiler.err.cant.apply.symbols
    }

    void listGetWrongIndexType(List<String> names) {
        names.get("0"); // error: compiler.err.cant.apply.symbol
    }

    void mapPutWrongKeyType(Map<String, Integer> counts) {
        counts.put(1, 1); // error: compiler.err.cant.apply.symbol
    }

    void boxSetWrongType(GenericApi.Box<String> box) {
        box.set(1); // error: compiler.err.cant.apply.symbol
    }

    void inheritedGenericSetWrongType(GenericApi.StringBox box) {
        box.set(1); // error: compiler.err.cant.apply.symbol
    }

    void interfaceDefaultWrongType(GenericApi.Holder<String> holder) {
        holder.orDefault(1); // error: compiler.err.cant.apply.symbol
    }

    void typeParameterBoundViolated() {
        GenericApi.num("x"); // error: compiler.err.cant.apply.symbol
    }

    void wildcardBoundViolated(List<String> names) {
        GenericApi.sum(names); // error: compiler.err.cant.apply.symbol
    }

    void invariantListRejected(List<Integer> integers) {
        GenericApi.sumExact(integers); // error: compiler.err.cant.apply.symbol
    }

    void recursiveBoundViolated() {
        GenericApi.maxOf(new Object(), new Object()); // error: compiler.err.cant.apply.symbol
    }

    void inferredValueTypeConflicts(Map<String, Integer> counts) {
        GenericApi.put(counts, "key", "value"); // error: compiler.err.cant.apply.symbol
    }

    void genericReturnAsWrongArgument(GenericApi.Box<Integer> box) {
        takesString(box.get()); // error: compiler.err.cant.apply.symbol
    }

    void optionalGetAsWrongArgument(Optional<Integer> number) {
        takesString(number.get()); // error: compiler.err.cant.apply.symbol
    }

    void rawElementIsObject(List raw) {
        takesString(raw.get(0)); // error: compiler.err.cant.apply.symbol
    }

    void chainResultAsWrongArgument(List<String> names) {
        takesString(names.get(0).length()); // error: compiler.err.cant.apply.symbol
    }
}
