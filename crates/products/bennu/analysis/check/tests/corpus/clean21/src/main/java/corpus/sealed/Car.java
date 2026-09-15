package corpus.sealed;

/** A final permitted subclass of {@link Vehicle}. */
public final class Car extends Vehicle {

    private final int seats;

    public Car(String plate, int seats) {
        super(plate);
        this.seats = seats;
    }

    public int seats() {
        return seats;
    }

    @Override
    public int wheels() {
        return 4;
    }
}
