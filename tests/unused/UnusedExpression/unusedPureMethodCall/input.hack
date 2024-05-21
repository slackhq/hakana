final class UnusedMethodClass {
    public function getId()[]: int {
		return 5;
	}

	public function doWork(): string {
		return '';
	}
}

function foo(): void {
    $c = new UnusedMethodClass();
    $c->getId();
    $c->doWork();
}