newtype decoded_id_t as int = int;

const decoded_id_t SLACKBOT_USER_ID = 1;

type message_t = shape(
	'user_id' => decoded_id_t,
	'text' => string,
);
