package corpus.service;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Optional;

import corpus.service.DelegateTypes.Delegate;
import corpus.service.DelegateTypes.DelegateClient;
import corpus.service.DelegateTypes.DelegateException;
import corpus.service.DelegateTypes.DelegateResolver;
import corpus.service.DelegateTypes.DelegateToken;

/**
 * The reported service shape at level 8: a resolver returning {@code Optional<Delegate>}, a presence
 * guard, {@code get()} chains, and a three-argument client call written with leading commas.
 */
public class DelegateService {

    private final DelegateResolver resolver;
    private final DelegateClient client;
    private final Map<String, DelegateToken> token_cache = new HashMap<String, DelegateToken>();

    public DelegateService(DelegateResolver resolver, DelegateClient client) {
        this.resolver = resolver;
        this.client = client;
    }

    public boolean check_delegate(final String username) throws DelegateException {
        Optional<Delegate> delegate_opt = resolver.resolve_delegate(username);

        if (!delegate_opt.isPresent())
            return false;

        Delegate delegate = delegate_opt.get();

        Optional<DelegateToken> delegate_db_opt =
            client.delegate_for_user(
                username
                , delegate.identifier()
                , delegate_opt.get()
            );

        if (delegate_db_opt.isPresent()) {
            token_cache.put(username, delegate_db_opt.get());
        }
        return delegate_db_opt.isPresent();
    }

    public Optional<String> cached_token(String username, long now) {
        DelegateToken token = token_cache.get(username);
        if (token == null || token.expired(now)) {
            return Optional.empty();
        }
        return Optional.of(token.value());
    }

    public String describe(final String username) {
        return resolver.resolve_delegate(username)
            .map(Delegate::display_name)
            .filter(name -> !name.isEmpty())
            .orElseGet(() -> "no delegate for " + username);
    }

    public String identifier_or_default(String username) {
        Optional<Delegate> delegate_opt = resolver.resolve_delegate(username);
        return delegate_opt.isPresent() ? delegate_opt.get().identifier() : username;
    }

    public List<String> check_all(List<String> usernames) {
        List<String> failures = new ArrayList<String>();
        for (String username : usernames) {
            try {
                if (!check_delegate(username)) {
                    failures.add(username);
                }
            } catch (DelegateException e) {
                failures.add(username + ": " + e.getMessage());
            }
        }
        return failures;
    }

    public static DelegateService offline(final Delegate fixed) {
        DelegateResolver resolver = username -> Optional.of(fixed);
        DelegateClient client = (username, identifier, delegate) ->
            identifier.equals(delegate.identifier())
                ? Optional.of(new DelegateToken(username + ":" + identifier, Long.MAX_VALUE))
                : Optional.<DelegateToken>empty();
        return new DelegateService(resolver, client);
    }
}
