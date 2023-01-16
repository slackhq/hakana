namespace Hakana\SecurityAnalysis;

/**
 * Used to denote a the shape fields that may contain dangerous taints.
 *
 * Any function param that specifies the given array shape type alias will
 * have these taints added automatically.
 */
final class ShapeSource implements \HH\TypeAliasAttribute {
	public function __construct(public dict<string, string> $map) {}
}
