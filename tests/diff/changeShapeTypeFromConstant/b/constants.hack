namespace Foo;

enum MyType: string {
	INT = 'int';
	STRING = 'string';
}

<<\Hakana\ShapeTypeFromConstant('MY_KEYS', dict[])>>
type my_keys_t = dict<string, mixed>;

const dict<string, MyType> MY_KEYS = dict[
	'a' => MyType::INT,
	'b' => MyType::STRING,
	'c' => MyType::INT,  // Added this key
];
