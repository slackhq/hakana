final class Reference {
  private function __construct(private int $id)[] {}

  public static function build(int $id)[]: this {
    return new static($id);
  }

  public function getId(): int {
    return $this->id;
  }
}

// enum class whose values are built with a static method call instead of `new`
enum class References: Reference {
  Reference First = Reference::build(1);
  Reference Second = Reference::build(2);
}

function takesReference(HH\MemberOf<References, Reference> $reference): int {
  return $reference->getId();
}

function foo(): int {
  return takesReference(References::First);
}
