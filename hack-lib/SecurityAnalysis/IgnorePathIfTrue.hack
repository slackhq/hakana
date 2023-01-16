namespace Hakana\SecurityAnalysis;

/**
 * Hakana will stop creating paths whenever a function with this attribute returns true.
 *
 * e.g.
 * if (is_ok_to_do_something_dangerous()) {
 *     echo $dangerous_value; // this issue is ignored
 * }
 * echo $dangerous_value; // this issue is caught
 *
 */
final class IgnorePathIfTrue implements \HH\FunctionAttribute, \HH\MethodAttribute {
	public function __construct() {}
}
