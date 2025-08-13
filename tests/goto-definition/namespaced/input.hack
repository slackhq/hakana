namespace MyNamespace {
    class NamespacedClass {
        public function method(): string {
            return "namespaced";
        }
    }
}

namespace AnotherNamespace {
    use MyNamespace\NamespacedClass;
    
    function test_namespaced(): void {
        $obj = new NamespacedClass();
        $obj->method();
    }
}