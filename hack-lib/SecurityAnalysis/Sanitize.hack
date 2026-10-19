namespace Hakana\SecurityAnalysis;

/**
 * Used to denote a function or method that removes taints from its input
 * in a manner that Hakana does not otherwise comprehend.
 */
final class Sanitize implements \HH\FunctionAttribute, \HH\MethodAttribute, \HH\ParameterAttribute {
	public function __construct(string ...$types) {}
}
