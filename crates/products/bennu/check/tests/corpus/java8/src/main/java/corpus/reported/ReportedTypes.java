package corpus.reported;

import java.util.Optional;

/** The types of the reported delegate-lookup method, Lombok-free. Must stay error-free. */
public final class ReportedTypes {
    private ReportedTypes() {
    }

    public static class Delegate {
        public String identifier() {
            return "id";
        }
    }

    public static class Token {
    }

    public interface IdentityResolver {
        Optional<Delegate> resolveIdentity();
    }

    /** Third parameter is a {@link Token}: passing the {@link Delegate} is the reported mistake. */
    public interface TokenClient {
        Optional<Token> delegateForUser(String username, String identifier, Token token);
    }

    /** Third parameter is a {@link Delegate}: the same call compiles. */
    public interface DelegateClient {
        Optional<Token> delegateForUser(String username, String identifier, Delegate delegate);
    }
}
