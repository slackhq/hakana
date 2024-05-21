final class UnusedMethodClass {
    <<Hakana\MustUse>>
	public function getEncodedId(): string {
		return '';
	}

	public function doWork(): string {
		return '';
	}
}

function foo(): void {
    $c = new UnusedMethodClass();
    $c->getEncodedId();
    $c->doWork();
}