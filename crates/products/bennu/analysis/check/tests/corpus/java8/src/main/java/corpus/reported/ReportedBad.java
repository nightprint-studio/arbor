package corpus.reported;

import java.util.Optional;

import corpus.reported.ReportedTypes.Delegate;
import corpus.reported.ReportedTypes.Token;

/**
 * The reported method with explicit types instead of Lombok's {@code val}: the third argument is
 * the {@code Optional}'s {@link Delegate} where the client expects a {@link Token}. Written once on
 * one line and once formatted as reported, where javac and the checker may point at different lines
 * of the same statement. Twin: {@link ReportedOk}.
 */
public class ReportedBad {
    private final ReportedTypes.IdentityResolver resolver;
    private final ReportedTypes.TokenClient client;

    ReportedBad(ReportedTypes.IdentityResolver resolver, ReportedTypes.TokenClient client) {
        this.resolver = resolver;
        this.client = client;
    }

    boolean checkDelegate(final String username) {
        Optional<Delegate> opt = resolver.resolveIdentity();
        if (!opt.isPresent()) {
            return false;
        }
        Delegate d = opt.get();
        Optional<Token> db = client.delegateForUser(username, d.identifier(), opt.get()); // error: compiler.err.cant.apply.symbol
        return db.isPresent();
    }

    boolean checkDelegateAsFormatted(final String username) {
        Optional<Delegate> delegateOpt = resolver.resolveIdentity();

        if (!delegateOpt.isPresent())
            return false;

        Delegate delegate = delegateOpt.get();

        Optional<Token> delegateDbOpt =
            client.delegateForUser( // error: compiler.err.cant.apply.symbol
                username
                , delegate.identifier()
                , delegateOpt.get()
            );

        return delegateDbOpt.isPresent();
    }
}
