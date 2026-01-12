namespace Foo;

enum MyType: string {
	INT = 'int';
	STRING = 'string';
}

<<\Hakana\ShapeKeysFromConstant('MY_KEYS')>>
type my_keys_t = dict<string, mixed>;

const dict<string, MyType> MY_KEYS = dict[
	'a' => MyType::INT,
	'b' => MyType::STRING,
];