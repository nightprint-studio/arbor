package corpus.generics;

/** Nested classes named like type variables: no other file sees them by their simple name. */
class TypeVariableFixtures {
    static class A {
    }

    static class D {
        void onlyHere() {
        }
    }
}
