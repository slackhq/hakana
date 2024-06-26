// Some class definitions to make a more involved example
interface IHasName {
  public function name(): string;
}

final class HasName implements IHasName {
  public function __construct(private string $name)[] {}
  public function name(): string {
    return $this->name;
  }
}

final class ConstName implements IHasName {
  public function name(): string {
    return "bar";
  }
}

// enum class which base type is the IHasName interface: each enum value
// can be any subtype of IHasName, here we see HasName and ConstName
enum class Names: IHasName {
  HasName Hello = new HasName('hello');
  HasName World = new HasName('world');
  ConstName Bar = new ConstName();
}

function takesNames(HH\MemberOf<Names, IHasName> $n): string {
    return $n->name();
}

function foo(): void {
    takesNames(Names::Hello);
}