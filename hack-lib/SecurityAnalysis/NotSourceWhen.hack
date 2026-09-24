namespace Hakana\SecurityAnalysis;

/**
 * A source contract, not a sanitizer. When the named argument is known to be
 * one of these exact strings, the call introduces no new taint source.
 * Taint propagated from other arguments is preserved. Unknown keys stay sources.
 */
final class NotSourceWhen implements \HH\FunctionAttribute, \HH\MethodAttribute {
	public function __construct(string $_parameter, string ...$_values) {}
}
