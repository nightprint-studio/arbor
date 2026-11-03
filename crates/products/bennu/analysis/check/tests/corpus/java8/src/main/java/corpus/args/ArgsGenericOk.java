package corpus.args;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/**
 * Legal twins of {@link ArgsGenericBad}: the same shapes with the parameterization right, and the ones that
 * only look wrong — a wildcard, a subtype's own parameterization, and a generic method whose type variable
 * is fixed by the parameter it is passed to.
 */
public class ArgsGenericOk {
    static class Animal {
    }

    static class Dog extends Animal {
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

    /** A generic method: what it returns is decided by the parameter it is passed to. */
    static <T> Page<T> empty() {
        return new Page<T>();
    }

    void takesDogs(Page<Dog> page) {
    }

    void takesSomeAnimals(Page<? extends Animal> page) {
    }

    void takesAnything(Page<?> page) {
    }

    void takesAnimalList(List<? extends Animal> animals) {
    }

    void theSameParameterization() {
        takesDogs(dogs());
        takesDogs(new Page<Dog>());
        takesDogs(new Repo().all());
        this.takesDogs(dogs());
    }

    void aWildcardAcceptsTheSubtypesParameterization(Page<Dog> page) {
        takesSomeAnimals(page);
        takesSomeAnimals(dogs());
        takesAnything(page);
        takesAnything(new Repo().all());
    }

    void aGenericMethodTakesItsTypeFromTheParameter() {
        takesDogs(empty());
        takesDogs(ArgsGenericOk.<Dog>empty());
    }

    void aListOfASubtypeFitsABoundedWildcard() {
        List<Dog> dogs = new ArrayList<Dog>();
        takesAnimalList(dogs);
        takesAnimalList(Collections.<Dog>emptyList());
    }
}
