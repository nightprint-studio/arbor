package corpus.service;

import java.util.Optional;

/** The types behind the reported delegate lookup, Lombok-free and at level 8. */
public final class DelegateTypes {

    private DelegateTypes() {
    }

    public static final class Delegate {
        private final String identifier;
        private final String display_name;

        public Delegate(String identifier, String display_name) {
            this.identifier = identifier;
            this.display_name = display_name;
        }

        public String identifier() {
            return identifier;
        }

        public String display_name() {
            return display_name;
        }
    }

    public static final class DelegateToken {
        private final String value;
        private final long expires_at;

        public DelegateToken(String value, long expires_at) {
            this.value = value;
            this.expires_at = expires_at;
        }

        public String value() {
            return value;
        }

        public boolean expired(long now) {
            return expires_at <= now;
        }
    }

    public interface DelegateResolver {
        Optional<Delegate> resolve_delegate(String username);
    }

    public interface DelegateClient {
        Optional<DelegateToken> delegate_for_user(String username, String identifier, Delegate delegate)
                throws DelegateException;
    }

    public static class DelegateException extends Exception {
        private static final long serialVersionUID = 1L;

        public DelegateException(String message) {
            super(message);
        }
    }
}
