namespace Hakana\SecurityAnalysis;

/**
 * Used to denote a source in taint/security analysis. It can have one or more taint types.
 */
final class Source implements \HH\FunctionAttribute, \HH\MethodAttribute {
	public function __construct(string ...$types) {}
}
