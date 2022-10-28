type MyShape = shape('foo' => string, ...);
function takes_shape(MyShape $_): void {}

function main(dict<string, mixed> $d) {
  takes_shape($d as MyShape);
}