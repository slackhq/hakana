<<Hakana\TypeAssertion('$missing', 'T')>>
function unknown_parameter<T>(mixed $value): void {}

<<Hakana\TypeAssertion('$value', 'Missing')>>
function unknown_type<T>(mixed $value): void {}

<<Hakana\TypeAssertion('$value')>>
function missing_argument<T>(mixed $value): void {}

<<Hakana\TypeAssertion('$value', 123)>>
function nonliteral_type<T>(mixed $value): void {}

<<Hakana\TypeAssertion('$value', 'T', 'extra')>>
function extra_argument<T>(mixed $value): void {}

<<Hakana\TypeAssertion('$value', 'T')>>
function variadic<T>(mixed ...$value): void {}

<<Hakana\TypeAssertion('$value', 'T')>>
function inout_parameter<T>(inout mixed $value): void {}
