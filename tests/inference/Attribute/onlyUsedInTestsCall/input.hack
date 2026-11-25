<<Hakana\OnlyUsedInTests>>
function soft(): void {}

<<Hakana\TestOnly>>
function hard(): void {}

<<Hakana\OnlyUsedInTests>>
function soft_caller(): void {
	soft();
	hard();
}

<<Hakana\OnlyUsedInTests>>
class Soft {
	public function __construct() {}

	public function run(): void {}

	<<Hakana\TestOnly>>
	public function fake(): void {}
}

final class SoftChild extends Soft {}

<<Hakana\TestOnly>>
final class Hard {
	<<Hakana\OnlyUsedInTests>>
	public static function run(): void {}
}

final class Prod {
	<<Hakana\OnlyUsedInTests>>
	public static function soft(): void {}

	<<Hakana\TestOnly>>
	public static function hard(): void {}
}

<<__EntryPoint>>
function main(): void {
	soft();
	hard();
	$soft = new Soft();
	$soft->run();
	$soft->fake();
	(new SoftChild())->run();
	Hard::run();
	Prod::soft();
	Prod::hard();
	test_helper();
}
