package corpus.resolve;

import java.util.NoSuchClassHere; // error: compiler.err.cant.resolve.location
import com.nowhere.at.all.Thing; // error: compiler.err.doesnt.exist
import com.nowhere.wildcard.*; // error: compiler.err.doesnt.exist
import static java.lang.Math.noSuchMethod; // error: compiler.err.cant.resolve.location
import static java.util.NoSuchType.member; // error: compiler.err.cant.resolve.location compiler.err.static.imp.only.classes.and.interfaces

/** Imports that resolve to nothing; nothing below uses them, so each fails exactly once. Twin: {@link ResolveImportOk}. */
public class ResolveImportBad {
}
