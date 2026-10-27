package corpus.records;

/** A local record is implicitly static: it reaches neither the enclosing locals nor the enclosing instance. Twin: {@link RecordShapesOk}. */
public class RecordLocalBad {
    int instanceField;

    void capturesALocal() {
        int base = 1;
        record Local(int x) {
            int plus() {
                return x + base; // error: compiler.err.non-static.cant.be.ref
            }
        }
        System.out.println(new Local(1).plus());
    }

    void readsAnInstanceField() {
        record Local(int x) {
            int plus() {
                return x + instanceField; // error: compiler.err.non-static.cant.be.ref
            }
        }
        System.out.println(new Local(1).plus());
    }
}
