abstract class ParentClass {}
final class ChildClass extends ParentClass {}

function takes_shape(shape('field1' => ChildClass, 'field2' => int) $s): void {}

function test(ParentClass $p): void {
    // This will trigger LessSpecificArgument since ParentClass is less specific than ChildClass
    takes_shape(shape('field1' => $p, 'field2' => 123));
}

function returns_wrong_shape(ParentClass $p): shape('x' => ChildClass, 'y' => int) {
    // This will trigger LessSpecificReturnStatement
    return shape('x' => $p, 'y' => 456);
}
