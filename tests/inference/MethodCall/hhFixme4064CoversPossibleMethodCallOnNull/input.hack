final class FixmeTarget {
	public function call(): void {}
}

final class FixmeCaller {
	private ?FixmeTarget $suppressed = null;
	private ?FixmeTarget $reported = null;

	public function call(): void {
		$this->reported->call();
		/* HH_FIXME[4064] */
		$this->suppressed->call();
	}
}
