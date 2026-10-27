package corpus.access;

import corpus.access.lib.Shown;

/** What another package may use: public members, and protected ones through inheritance. Twin of {@link AccessBad}. */
public class AccessOk extends Shown {
    int viaPublic(Shown shown) {
        shown.visible();
        return shown.open;
    }

    int viaInheritance() {
        protectedMethod();
        return guarded;
    }
}
