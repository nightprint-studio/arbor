package corpus.generics;

import java.util.ArrayList;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Generic members inherited and substituted through two and more levels, overrides of substituted
 * signatures (javac emits bridges for them), covariant returns in classes and interfaces.
 */
public final class GenericHierarchy {

    private GenericHierarchy() {
    }

    public interface Named {
        String name();
    }

    public interface Repository<T, ID> {
        T find(ID id);

        List<T> findAll();

        void save(T entity);
    }

    public abstract static class AbstractRepository<T, ID> implements Repository<T, ID> {
        protected final Map<ID, T> store = new LinkedHashMap<ID, T>();

        protected abstract ID idOf(T entity);

        @Override
        public T find(ID id) {
            return store.get(id);
        }

        @Override
        public List<T> findAll() {
            return new ArrayList<T>(store.values());
        }

        @Override
        public void save(T entity) {
            store.put(idOf(entity), entity);
        }
    }

    public abstract static class NamedRepository<N extends Named> extends AbstractRepository<N, String> {
        @Override
        protected String idOf(N entity) {
            return entity.name();
        }

        public N findByName(String name) {
            return find(name);
        }
    }

    public static final class City implements Named, Comparable<City> {
        private final String name;
        private final int population;

        public City(String name, int population) {
            this.name = name;
            this.population = population;
        }

        @Override
        public String name() {
            return name;
        }

        public int population() {
            return population;
        }

        @Override
        public int compareTo(City other) {
            return Integer.compare(population, other.population);
        }
    }

    /** {@code find(String)} overrides {@code T find(ID)} of a grandparent, substituted through the parent. */
    public static final class CityRepository extends NamedRepository<City> {
        @Override
        public City find(String id) {
            City city = super.find(id);
            return city != null ? city : new City(id, 0);
        }

        public int totalPopulation() {
            int total = 0;
            for (City city : findAll()) {
                total += city.population();
            }
            return total;
        }
    }

    public interface Source<T> {
        T next();
    }

    /** Covariant override of a method inherited from a parameterized superinterface. */
    public interface TextSource extends Source<CharSequence> {
        @Override
        String next();
    }

    public static final class Cursor implements TextSource {
        private int position;

        @Override
        public String next() {
            position++;
            return "item" + position;
        }
    }

    public abstract static class Animal implements Cloneable {
        public abstract Animal mate();

        @Override
        protected Animal clone() throws CloneNotSupportedException {
            return (Animal) super.clone();
        }
    }

    public static final class Dog extends Animal {
        @Override
        public Dog mate() {
            return new Dog();
        }

        @Override
        public Dog clone() throws CloneNotSupportedException {
            return (Dog) super.clone();
        }
    }

    public static String demo() throws CloneNotSupportedException {
        CityRepository repository = new CityRepository();
        repository.save(new City("Rome", 2800000));
        repository.save(new City("Milan", 1400000));
        Repository<City, String> generic = repository;
        NamedRepository<City> named = repository;
        City rome = named.findByName("Rome");
        City milan = generic.find("Milan");
        City largest = Collections.max(repository.findAll());
        Source<CharSequence> source = new Cursor();
        CharSequence first = source.next();
        String second = new Cursor().next();
        Dog puppy = new Dog().mate().clone();
        return rome.name() + milan.name() + largest.name() + repository.totalPopulation()
                + first + second + puppy.mate();
    }
}
