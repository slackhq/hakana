namespace Hakana\FindPaths;

/**
 * This annotation marks find-paths sinks as sanitized.
 */
final class Sanitize implements \HH\FunctionAttribute, \HH\MethodAttribute {
	public function __construct(string ...$types) {}
}
