abstract class GenericContainer {
  abstract const type TValue as nonnull;
  
  public function __construct(
    protected this::TValue $value,
  ) {}
}

final class StringContainer extends GenericContainer {
  const type TValue = string;
 
  public function foo(): string {
    return $this->value;
  }
}