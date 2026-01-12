namespace ComparisonTest;

enum LogType: string {
	INT = 'int';
	STRING = 'string';
	BOOL = 'bool';
}

// Traditional ShapeTypeFromConstant - enforces both keys and value types
<<\Hakana\ShapeTypeFromConstant('TYPED_KEYS', dict['int' => 'int', 'string' => 'string', 'bool' => 'bool'])>>
type typed_shape_t = dict<string, mixed>;

// New ShapeKeysFromConstant - only enforces keys, ignores value types
<<\Hakana\ShapeKeysFromConstant('TYPED_KEYS')>>
type keys_only_shape_t = dict<string, mixed>;

const dict<string, LogType> TYPED_KEYS = dict[
	'user_id' => LogType::INT,
	'username' => LogType::STRING, 
	'is_active' => LogType::BOOL,
];

function test_type_enforcement(typed_shape_t $typed_data): void {
	// With ShapeTypeFromConstant, these would enforce value types:
	// $typed_data would expect 'user_id' => int, 'username' => string, etc.
	echo $typed_data['user_id'];    // Expected to be int
	echo $typed_data['username'];   // Expected to be string
	echo $typed_data['is_active'];  // Expected to be bool
}

function test_keys_only(keys_only_shape_t $keys_data): void {
	// With ShapeKeysFromConstant, these only check key existence:
	echo $keys_data['user_id'];     // Any type allowed
	echo $keys_data['username'];    // Any type allowed
	echo $keys_data['is_active'];   // Any type allowed
	echo $keys_data['__debug'];     // Allowed due to __ prefix
	
	// This should still fail - invalid key:
	echo $keys_data['invalid_key']; // ERROR: key not in TYPED_KEYS
}