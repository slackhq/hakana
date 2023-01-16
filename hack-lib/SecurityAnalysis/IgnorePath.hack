namespace Hakana\SecurityAnalysis;

/**
 * This annotation prevents taints from flowing through any functions or methods
 * it's annotated with.
 */
final class IgnorePath implements \HH\FunctionAttribute, \HH\MethodAttribute {
	public function __construct() {}
}
