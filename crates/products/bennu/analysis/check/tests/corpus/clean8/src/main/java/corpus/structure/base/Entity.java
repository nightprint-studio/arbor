package corpus.structure.base;

/** A base class in another package: protected constructor, protected field and method. */
public abstract class Entity<ID extends Comparable<ID>> implements Comparable<Entity<ID>> {

    private final ID id;
    protected int version;

    protected Entity(ID id) {
        this.id = id;
    }

    public final ID getId() {
        return id;
    }

    protected void touch() {
        version++;
    }

    public abstract String kind();

    @Override
    public int compareTo(Entity<ID> other) {
        return id.compareTo(other.id);
    }

    @Override
    public String toString() {
        return kind() + "#" + id + "v" + version;
    }
}
