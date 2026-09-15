package corpus.generic;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;

/** Generic types given the wrong number of type arguments. Twin: {@link GenericArityOk}. */
public class GenericArityBad {
    static class Pair<A, B> {
    }

    void tooManyForAJdkType() {
        List<String, String> values = null; // error: compiler.err.wrong.number.type.args
    }

    void tooFewForAJdkType() {
        Map<String> values = null; // error: compiler.err.wrong.number.type.args
    }

    void tooFewForAProjectType() {
        Pair<String> pair = null; // error: compiler.err.wrong.number.type.args
    }

    void tooManyInACreation() {
        Object values = new ArrayList<String, String>(); // error: compiler.err.wrong.number.type.args
    }

    void argumentsForANonGenericJdkType() {
        String<Integer> text = null; // error: compiler.err.type.doesnt.take.params
    }

    void argumentsForANonGenericProjectType() {
        GenericArityBad<String> self = null; // error: compiler.err.type.doesnt.take.params
    }
}
