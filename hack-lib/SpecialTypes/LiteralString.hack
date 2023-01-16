namespace Hakana\SpecialTypes;

/**
 * Used to denote a literal string in Hakana.
 *
 * Add this to any type alias where you want Hakana to treat it
 * as a literal string
 */
final class LiteralString implements \HH\TypeAliasAttribute {
	public function __construct() {}
}
