
abstract class ParentClass {
	abstract public function doThing(): void;
}

trait ParentTrait {
	public function doThing(): void {}
}

final class ConcreteClass extends ParentClass {
	use ParentTrait;
}

final class UsesClass {
	public function __construct() {
		$resp = new ConcreteClass();
    	$this->process($resp);
	}

	private function process(ParentClass $response): void {
		$response->doThing();
	}
}

<<__EntryPoint>>
function main(): void {
    new UsesClass();
}