package corpus.scope;

import java.util.Arrays;

import static corpus.scope.ScopeTypes.Util.twice;
import static java.util.Arrays.asList;

/** Legal twins of {@link ScopeBad}, plus members reached through enclosing and inherited scopes. */
public class ScopeOk {
    private int count = 1;
    private String label = "label";

    void takesInt(int value) {
    }

    void takesString(String value) {
    }

    void helper(int value) {
    }

    private void outerPrivate(int value) {
    }

    private static void outerPrivateStatic(int value) {
    }

    class InnerWithSameName {
        void helper() {
        }

        void call() {
            helper();
            ScopeOk.this.helper(1);
        }
    }

    static class NestedCallsOuterPrivateStatic {
        void call() {
            outerPrivateStatic(1);
        }
    }

    class InnerCallsOuterPrivate {
        void call() {
            outerPrivate(1);
        }
    }

    static class SubWithHelper extends ScopeTypes.Base {
        class Helper {
            void go() {
                inherited("x");
            }
        }
    }

    static class LimitsUser implements ScopeTypes.Limits {
        int limit() {
            return MAX;
        }
    }

    void inheritedCall(ScopeTypes.Sub sub) {
        sub.inherited("x");
    }

    void inheritedStatic() {
        ScopeTypes.Sub.staticInherited(1);
    }

    void defaultMethod(ScopeTypes.Greeter greeter) {
        greeter.greet("x");
        greeter.name();
    }

    void interfaceStatic() {
        ScopeTypes.Greeter.of("x");
    }

    void staticCalls() {
        ScopeTypes.Util.twice(2);
        ScopeTypes.Util.join("left", "right");
    }

    void staticImports() {
        twice(2);
        asList(1, 2);
    }

    void qualifiedWhereAStaticImportWouldBeShadowed() {
        String text = Arrays.toString(new int[] { 1 });
    }

    void nestedTypes(ScopeTypes.Outer outer) {
        new ScopeTypes.Outer.Nested(1).work("x");
        outer.new Inner().work(1);
    }

    void localShadowsFieldWithAnotherType() {
        int label = 1;
        takesInt(label);
        takesString(this.label);
    }

    void parameterShadowsField(String count) {
        takesString(count);
        takesInt(this.count);
    }

    void anonymousClassInherited() {
        new ScopeTypes.Base() { void use() { inherited("x"); } };
    }

    void enumValueOf() {
        ScopeTypes.Level.valueOf("LOW");
        Enum.valueOf(ScopeTypes.Level.class, "HIGH");
    }

    static void anonymousClassInAStaticMethod() {
        new Runnable() { public void run() { outerPrivateStatic(1); } };
    }
}
