namespace Hakana;

/**
 * Used to denote a function that's only meant to be used in tests.
 * Everything in hakana.json's test_files field is already considered test-only.
 * This attribute should be used when test functions/classes live in the same file
 * as production functions & classes
 */
final class TestOnly implements \HH\FunctionAttribute, \HH\ClassAttribute, \HH\MethodAttribute {
	public function __construct() {}
}
