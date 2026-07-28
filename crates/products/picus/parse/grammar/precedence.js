// The one precedence ladder, shared by every module that builds an expression.
//
// `between` sits ABOVE `and` on purpose: in `a BETWEEN b AND c` the parser sees
// `AND` with two productions available, and the higher number is what makes it
// finish the BETWEEN instead of starting a boolean conjunction.
module.exports = {
  PREC: {
    or: 1,
    and: 2,
    not: 3,
    compare: 4,
    between: 5,
    pg_op: 6,
    concat: 7,
    add: 8,
    mul: 9,
    unary: 11,
    cast: 13,
    postfix: 14,
  },
};
