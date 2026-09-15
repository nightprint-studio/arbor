package corpus.records;

import java.math.BigDecimal;
import java.math.RoundingMode;
import java.util.Currency;
import java.util.Objects;

/** A top-level record: compact canonical constructor normalizing a component, a delegating constructor, factories. */
public record Money(BigDecimal amount, Currency currency) implements Comparable<Money> {

    public static final Money ZERO_EUR = of("0", "EUR");

    public Money {
        Objects.requireNonNull(amount, "amount");
        Objects.requireNonNull(currency, "currency");
        amount = amount.setScale(currency.getDefaultFractionDigits(), RoundingMode.HALF_EVEN);
    }

    public Money(long cents, String currencyCode) {
        this(BigDecimal.valueOf(cents, 2), Currency.getInstance(currencyCode));
    }

    public static Money of(String amount, String currencyCode) {
        return new Money(new BigDecimal(amount), Currency.getInstance(currencyCode));
    }

    public static Money sum(Iterable<Money> values, Currency currency) {
        var total = new Money(BigDecimal.ZERO, currency);
        for (var value : values) {
            total = total.plus(value);
        }
        return total;
    }

    public Money plus(Money other) {
        requireSameCurrency(other);
        return new Money(amount.add(other.amount), currency);
    }

    public Money times(int factor) {
        return new Money(amount.multiply(BigDecimal.valueOf(factor)), currency);
    }

    public boolean isZero() {
        return amount.signum() == 0;
    }

    private void requireSameCurrency(Money other) {
        if (!currency.equals(other.currency())) {
            throw new IllegalArgumentException("currency mismatch: %s vs %s".formatted(currency, other.currency()));
        }
    }

    @Override
    public int compareTo(Money other) {
        requireSameCurrency(other);
        return amount.compareTo(other.amount);
    }

    @Override
    public String toString() {
        return amount.toPlainString() + " " + currency.getCurrencyCode();
    }
}
