namespace Hakana;

final class TestOnly implements \HH\FunctionAttribute, \HH\ClassAttribute, \HH\MethodAttribute {
	public function __construct() {}
}

final class OnlyUsedInTests implements \HH\FunctionAttribute, \HH\ClassAttribute, \HH\MethodAttribute {
	public function __construct() {}
}
