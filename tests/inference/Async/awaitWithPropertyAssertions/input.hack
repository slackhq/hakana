final class B {
  public function bar(): ?string {
    return "a";
  }
}

final class A {
	private ?B $b;

	private async function convert(): Awaitable<void> {
		if ($this->b is nonnull) {
			await $this->b->bar() as nonnull;
		}
	}
}