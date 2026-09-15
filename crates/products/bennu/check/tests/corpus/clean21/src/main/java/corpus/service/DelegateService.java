package corpus.service;

import java.time.Instant;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.concurrent.ConcurrentHashMap;

import corpus.service.DelegateTypes.Delegate;
import corpus.service.DelegateTypes.DelegateClient;
import corpus.service.DelegateTypes.DelegateException;
import corpus.service.DelegateTypes.DelegateResolver;
import corpus.service.DelegateTypes.DelegateToken;

/**
 * The reported service shape at level 21: {@code var} locals, an {@code isEmpty()} guard, {@code get()}
 * chains, and a three-argument client call written with leading commas.
 */
public class DelegateService {

    private final DelegateResolver resolver;
    private final DelegateClient client;
    private final Map<String, DelegateToken> token_cache = new ConcurrentHashMap<>();

    public DelegateService(DelegateResolver resolver, DelegateClient client) {
        this.resolver = resolver;
        this.client = client;
    }

    public boolean check_delegate(final String username) throws DelegateException {
        var delegate_opt = resolver.resolve_delegate(username);

        if (delegate_opt.isEmpty())
            return false;

        var delegate = delegate_opt.get();

        var delegate_db_opt =
            client.delegate_for_user(
                username
                , delegate.identifier()
                , delegate_opt.get()
            );

        delegate_db_opt.ifPresent(token -> token_cache.put(username, token));
        return delegate_db_opt.isPresent();
    }

    public Optional<String> cached_token(String username, Instant now) {
        return Optional.ofNullable(token_cache.get(username))
            .filter(token -> !token.expired(now))
            .map(DelegateToken::value);
    }

    public String describe(String username) {
        return switch (resolver.resolve_delegate(username).orElse(null)) {
            case null -> "no delegate for " + username;
            case Delegate(var identifier, var display_name) when display_name.isBlank() -> identifier;
            case Delegate(var identifier, var display_name) -> display_name + " (" + identifier + ")";
        };
    }

    public List<String> check_all(List<String> usernames) {
        var failures = new ArrayList<String>();
        for (var username : usernames) {
            try {
                if (!check_delegate(username)) {
                    failures.add(username);
                }
            } catch (DelegateException e) {
                failures.add("%s: %s".formatted(username, e.getMessage()));
            }
        }
        return List.copyOf(failures);
    }

    public static DelegateService offline(Delegate fixed, Instant expires_at) {
        DelegateResolver resolver = username -> Optional.of(fixed);
        DelegateClient client = (username, identifier, delegate) ->
            identifier.equals(delegate.identifier())
                ? Optional.of(new DelegateToken(username + ":" + identifier, expires_at))
                : Optional.empty();
        return new DelegateService(resolver, client);
    }
}
