package corpus.lombok.model;

import lombok.extern.java.Log;

/** {@code @Log} declares the static {@code log} field the body then uses bare. */
@Log
public class Logged {
    public void hello(String who) {
        log.info("hello " + who);
    }
}
