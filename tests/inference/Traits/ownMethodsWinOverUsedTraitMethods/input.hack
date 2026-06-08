trait InnerTrait {
	public function getDisplayType(): ?string {
		return null;
	}
	public function getDisplayId(): ?string {
		return null;
	}
}

trait OuterTrait {
	use InnerTrait;

	// an outer trait's own method wins over the same method
	// from a trait it uses
	<<__Override>>
	public function getDisplayType(): string {
		return "issue";
	}
}

final class MyClass {
	use OuterTrait;

	// a class's own method wins over the same method from a used trait
	<<__Override>>
	public function getDisplayId(): string {
		return "id";
	}

	public function format(): shape('display_type' => string, 'display_id' => string) {
		return shape(
			'display_type' => $this->getDisplayType(),
			'display_id' => $this->getDisplayId(),
		);
	}
}
