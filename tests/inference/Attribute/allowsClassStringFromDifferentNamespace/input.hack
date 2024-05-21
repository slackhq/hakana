
namespace NamespaceOne {
    use Attribute;

    final class FooAttribute implements HH\ClassAttribute
    {
        public function __construct(private classname<FoobarInterface> $className)
        {}
    }

    interface FoobarInterface {}

    final class Bar implements FoobarInterface {}
}

namespace NamespaceTwo {
    use NamespaceOne\FooAttribute;
    use NamespaceOne\Bar as ZZ;

    <<FooAttribute(ZZ::class)>>
    final class Baz {}
}
                