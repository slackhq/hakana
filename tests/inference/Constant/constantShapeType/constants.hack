namespace Foo;

enum LogstashType: string {
	LONG = 'int';
	TEXT = 'string';
	FLOAT = 'float';
	BOOLEAN = 'bool';
	DATE = 'date';
	UNINDEXED = 'unindexed';
}

<<\Hakana\ShapeTypeFromConstant('ALLOWED_LOGSTASH_KEYS', dict['date' => 'string', 'unindexed' => 'string'])>>
type allowed_logstash_keys_t = dict<string, mixed>;

const dict<string, LogstashType> ALLOWED_LOGSTASH_KEYS = dict[
	'a' => LogstashType::TEXT,
	'b' => LogstashType::LONG,
	'c' => LogstashType::BOOLEAN,
];