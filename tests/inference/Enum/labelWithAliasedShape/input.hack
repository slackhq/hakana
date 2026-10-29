type Definition = shape('path' => string, 'field' => ?string);
type DefinitionLabel = HH\EnumClass\Label<Definitions, Definition>;
enum class Definitions: Definition {
  Definition ONE = shape('path' => 'example', 'field' => null);
}
type Config = shape('current' => DefinitionLabel, 'others' => vec<DefinitionLabel>);
final class Builder {
  public function __construct(public Config $config)[] {}
}
function example(): Builder {
  return new Builder(shape('current' => Definitions#ONE, 'others' => vec[]));
}
