function test_shapes_put_basic(): void {
    $s1 = shape('x' => 1);
    $s2 = Shapes::put($s1, 'y', 2);
    // $s2 should be shape('x' => int, 'y' => int)
    expect_shape_with_x_and_y($s2);
}

function expect_shape_with_x_and_y(shape('x' => int, 'y' => int) $s): void {}

function test_shapes_put_overwrite(): void {
    $s1 = shape('x' => 1, 'y' => 'hello');
    $s2 = Shapes::put($s1, 'y', 2);
    // $s2 should have 'y' as int now
    expect_shape_x_int_y_int($s2);
}

function expect_shape_x_int_y_int(shape('x' => int, 'y' => int) $s): void {}

function test_shapes_put_new_key(): void {
    $s1 = shape('a' => 'test');
    $s2 = Shapes::put($s1, 'b', true);
    expect_shape_a_str_b_bool($s2);
}

function expect_shape_a_str_b_bool(shape('a' => string, 'b' => bool) $s): void {}
