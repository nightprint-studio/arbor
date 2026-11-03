package corpus.args;

/**
 * Calls whose argument is the right class with the wrong PARAMETERIZATION. Generics are invariant: a
 * {@code Page<Dog>} is not a {@code Page<Cat>}, nor a {@code Page<Animal>}. Twin: {@link ArgsGenericOk}.
 */
public class ArgsGenericBad {
    static class Animal {
    }

    static class Dog extends Animal {
    }

    static class Cat extends Animal {
    }

    static class Page<T> {
    }

    static class Repo {
        Page<Dog> all() {
            return new Page<Dog>();
        }
    }

    static Page<Dog> dogs() {
        return new Page<Dog>();
    }

    void takesCats(Page<Cat> page) {
    }

    void takesAnimals(Page<Animal> page) {
    }

    void aBareCallGivenAStandaloneCall() {
        takesCats(dogs()); // error: compiler.err.cant.apply.symbol
    }

    void aBareCallGivenAVariable(Page<Dog> page) {
        takesCats(page); // error: compiler.err.cant.apply.symbol
    }

    void invariantNotCovariant() {
        takesAnimals(dogs()); // error: compiler.err.cant.apply.symbol
    }

    void aQualifiedCall() {
        this.takesCats(dogs()); // error: compiler.err.cant.apply.symbol
    }

    void theResultOfAnotherObjectsMethod() {
        Repo repo = new Repo();
        takesCats(repo.all()); // error: compiler.err.cant.apply.symbol
    }
}
