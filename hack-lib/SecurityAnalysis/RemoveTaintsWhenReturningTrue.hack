namespace Hakana\SecurityAnalysis;

/**
 * Used to denote a sink in taint/security analysis. It can have one or more taint types.
 */
final class RemoveTaintsWhenReturningTrue implements \HH\ParameterAttribute {
	public function __construct(string ...$types) {}
}
