
namespace NamespaceOne {
    use Attribute;

    class FooAttribute implements HH\ClassAttribute
    {
        public function __construct(private classname<FoobarInterface> $className)
        {}
    }

    interface FoobarInterface {}

    class Bar implements FoobarInterface {}
}

namespace NamespaceTwo {
    use NamespaceOne\FooAttribute;
    use NamespaceOne\Bar as ZZ;

    <<FooAttribute(ZZ::class)>>
    class Baz {}
}
                