package corpus.generic;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;

/** Legal twins of {@link GenericArityBad}. */
public class GenericArityOk {
    static class Pair<A, B> {
    }

    void exactTypeArguments() {
        List<String> values = null;
        Map<String, Integer> counts = null;
        Pair<String, Integer> pair = null;
    }

    void rawTypesTakeNoArguments() {
        List raw = null;
        Pair rawPair = null;
    }

    void diamonds() {
        List<String> values = new ArrayList<>();
        Pair<String, Integer> pair = new Pair<>();
    }

    void nestedTypeArguments() {
        Map<String, List<Pair<String, Integer>>> nested = null;
    }

    void wildcardArguments() {
        List<?> any = null;
        Map<? extends String, ? super Integer> bounded = null;
    }
}
