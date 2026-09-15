package corpus.inherit;

/** Sealed hierarchies and records extended illegally, and misplaced sealed/record modifiers. Twin: {@link InheritSealedOk}. */
public class InheritSealedBad {
    sealed interface Vehicle permits Car {
    }

    static final class Car implements Vehicle {
    }

    static final class Truck implements Vehicle { // error: compiler.err.cant.inherit.from.sealed
    }

    sealed interface Animal permits Dog {
    }

    static class Dog implements Animal { // error: compiler.err.non.sealed.sealed.or.final.expected
    }

    sealed static class NoSubclasses { // error: compiler.err.sealed.class.must.have.subclasses
    }

    non-sealed static class NoSealedSupertype { // error: compiler.err.non.sealed.with.no.sealed.supertype
    }

    sealed final static class SealedAndFinal { // error: compiler.err.illegal.combination.of.modifiers
    }

    record Point(int x) {
    }

    static class ExtendsARecord extends Point { // error: compiler.err.cant.inherit.from.final compiler.err.cant.apply.symbol
    }

    abstract record AbstractRecord(int x) { // error: compiler.err.mod.not.allowed.here
    }
}
