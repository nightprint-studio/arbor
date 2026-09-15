package corpus.service;

import java.time.Instant;
import java.util.Optional;

/** The types behind the reported delegate lookup, as records with snake_case components. */
public final class DelegateTypes {

    private DelegateTypes() {
    }

    public record Delegate(String identifier, String display_name) {
    }

    public record DelegateToken(String value, Instant expires_at) {
        public boolean expired(Instant now) {
            return !expires_at.isAfter(now);
        }
    }

    public interface DelegateResolver {
        Optional<Delegate> resolve_delegate(String username);
    }

    public interface DelegateClient {
        Optional<DelegateToken> delegate_for_user(String username, String identifier, Delegate delegate)
                throws DelegateException;
    }

    public static final class DelegateException extends Exception {
        private static final long serialVersionUID = 1L;

        public DelegateException(String message) {
            super(message);
        }
    }
}
