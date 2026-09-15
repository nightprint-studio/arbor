package corpus.scope;

import static corpus.scope.ScopeTypes.Util.twice;
import static java.util.Arrays.toString;

/** Bad arguments to members found through inheritance, static access, static imports and nesting. Twin: {@link ScopeOk}. */
public class ScopeBad {
    private int count = 1;

    void takesInt(int value) {
    }

    void helper(int value) {
    }

    class InnerWithSameName {
        void helper() {
        }

        void call() {
            helper(1); // error: compiler.err.cant.apply.symbol
        }
    }

    void inheritedWrongType(ScopeTypes.Sub sub) {
        sub.inherited(1); // error: compiler.err.cant.apply.symbol
    }

    void inheritedStaticWrongType() {
        ScopeTypes.Sub.staticInherited("x"); // error: compiler.err.cant.apply.symbol
    }

    void defaultMethodWrongType(ScopeTypes.Greeter greeter) {
        greeter.greet(1); // error: compiler.err.cant.apply.symbol
    }

    void interfaceStaticWrongType() {
        ScopeTypes.Greeter.of(1); // error: compiler.err.cant.apply.symbol
    }

    void staticCallWrongType() {
        ScopeTypes.Util.twice("x"); // error: compiler.err.cant.apply.symbol
    }

    void staticCallWrongArity() {
        ScopeTypes.Util.join("left"); // error: compiler.err.cant.apply.symbol
    }

    void staticImportWrongType() {
        twice("x"); // error: compiler.err.cant.apply.symbol
    }

    void staticImportShadowedByTheInheritedObjectMethod() {
        String text = toString(new int[] { 1 }); // error: compiler.err.cant.apply.symbol
    }

    void nestedConstructorWrongType() {
        new ScopeTypes.Outer.Nested("x"); // error: compiler.err.cant.apply.symbol
    }

    void nestedMethodWrongType() {
        new ScopeTypes.Outer.Nested(1).work(1); // error: compiler.err.cant.apply.symbol
    }

    void innerMethodWrongType(ScopeTypes.Outer outer) {
        outer.new Inner().work("x"); // error: compiler.err.cant.apply.symbol
    }

    void localShadowsFieldWithAnotherType() {
        String count = "x";
        takesInt(count); // error: compiler.err.cant.apply.symbol
    }

    void anonymousClassInheritedWrongType() {
        new ScopeTypes.Base() { void use() { inherited(1); } }; // error: compiler.err.cant.apply.symbol
    }

    void enumValueOfWrongType() {
        ScopeTypes.Level.valueOf(1); // error: compiler.err.cant.apply.symbols
    }
}
