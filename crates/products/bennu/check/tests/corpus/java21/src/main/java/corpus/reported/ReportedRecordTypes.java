package corpus.reported;

import java.util.Optional;

/** The reported delegate-lookup types as records — the closest level-21 shape to the Lombok original. */
public final class ReportedRecordTypes {
    private ReportedRecordTypes() {
    }

    public record Delegate(String identifier) {
    }

    public record Token(String value) {
    }

    public interface IdentityResolver {
        Optional<Delegate> resolveIdentity();
    }

    public interface TokenClient {
        Optional<Token> delegateForUser(String username, String identifier, Token token);
    }

    public interface DelegateClient {
        Optional<Token> delegateForUser(String username, String identifier, Delegate delegate);
    }
}
