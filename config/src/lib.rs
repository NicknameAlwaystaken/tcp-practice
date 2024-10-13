pub const PACKET_INFO_SIZE: usize = 2;

pub const PING_PACKET_SIZE: usize = 1;

pub const GAME_PACKET_VERSION: u8 = 1;
pub const GAME_PACKET_SIZE: usize = 2;

pub const AUTH_RESPONSE_VERSION: u8 = 1;
pub const AUTH_RESPONSE_SIZE: usize = 32;

pub const AUTH_REQUEST_VERSION: u8 = 1;
pub const AUTH_REQUEST_SIZE: usize = 52;

pub const USERNAME_LENGTH: usize = 20;
pub const PASSWORD_LENGTH: usize = 32;

#[derive(Debug)]
pub enum DataType {
    AuthRequest,
    AuthResponse,
    Ping,
    Disconnect,
    Unknown,
}

impl DataType {
    pub fn from_u8(value: u8) -> DataType {
        match value {
            1 => DataType::AuthRequest,
            2 => DataType::AuthResponse,
            3 => DataType::Ping,
            4 => DataType::Disconnect,
            _ => DataType::Unknown,
        }
    }
    pub fn to_u8(&self) -> u8 {
        match self {
            DataType::AuthRequest => 1,
            DataType::AuthResponse => 2,
            DataType::Ping => 3,
            DataType::Disconnect => 4,
            DataType::Unknown => 5,
        }
    }
}

pub fn get_package_type(bytes: [u8; 2]) -> (u8, u8, DataType) {
    let version: u8 = bytes[0];
    let encoding_and_type: u8 = bytes[1];
    let encoding: u8 = encoding_and_type >> 6;
    let data_type: DataType = DataType::from_u8(encoding_and_type & 0x3F);

    (version, encoding, data_type)
}

pub fn unpack_auth_request_package(bytes: &[u8; USERNAME_LENGTH + PASSWORD_LENGTH]) -> (String , String) {
    let (left, right) = bytes.split_at(USERNAME_LENGTH);
    let mut username_array = [0u8; USERNAME_LENGTH];
    let mut password_array = [0u8; PASSWORD_LENGTH];

    username_array.copy_from_slice(left);
    password_array.copy_from_slice(right);

    (String::from_utf8(username_array.to_vec()).unwrap(), String::from_utf8(password_array.to_vec()).unwrap())
}

pub fn create_auth_request_package(username: String, password: String) -> [u8; PACKET_INFO_SIZE + AUTH_REQUEST_SIZE] {
    let version: u8 = AUTH_REQUEST_VERSION;
    let encoding: u8 = 0;
    let encoding_and_data_type: u8 = (encoding << 6) as u8 | (DataType::AuthRequest.to_u8() & 0x3F);

    let mut response_array = [0u8; PACKET_INFO_SIZE + AUTH_REQUEST_SIZE];
    response_array[0] = version;
    response_array[1] = encoding_and_data_type;

    let username_bytes = username.as_bytes();
    let username_len = username_bytes.len().min(USERNAME_LENGTH);
    let username_field_start = PACKET_INFO_SIZE;
    let username_padding = USERNAME_LENGTH - username_len;
    let username_start_index = username_field_start + username_padding;

    response_array[username_start_index..username_start_index + username_len].copy_from_slice(&username_bytes[..username_len]);

    let password_bytes = password.as_bytes();
    let password_len = password_bytes.len().min(PASSWORD_LENGTH);
    let password_field_start = PACKET_INFO_SIZE + USERNAME_LENGTH;
    let password_padding = PASSWORD_LENGTH - password_len;
    let password_start_index = password_field_start + password_padding;

    response_array[password_start_index..password_start_index + password_len].copy_from_slice(&password_bytes[..password_len]);

    response_array
}

pub fn unpack_auth_response_package(bytes: &[u8; AUTH_RESPONSE_SIZE]) -> String {
    match String::from_utf8(bytes.to_vec()) {
        Ok(valid_string) => {
            return valid_string;
        }
        Err(_e) => {
            return '0'.to_string();
        }
    }
}

pub fn create_auth_response_package(token: String) -> [u8; PACKET_INFO_SIZE + AUTH_RESPONSE_SIZE] {
    let version: u8 = AUTH_RESPONSE_VERSION;
    let encoding: u8 = 0;
    let encoding_and_data_type: u8 = (encoding << 6) as u8 | (DataType::AuthResponse.to_u8() & 0x3F);

    let mut response_array = [0u8; PACKET_INFO_SIZE + AUTH_RESPONSE_SIZE];
    response_array[0] = version;
    response_array[1] = encoding_and_data_type;

    let token_bytes = token.as_bytes();
    let token_len = token_bytes.len().min(AUTH_RESPONSE_SIZE);
    let start_index = PACKET_INFO_SIZE + (AUTH_RESPONSE_SIZE - token_len);

    response_array[start_index..].copy_from_slice(token_bytes);

    response_array
}

pub fn create_empty_package(data_type: DataType) -> [u8; PACKET_INFO_SIZE] {
    let version: u8 = GAME_PACKET_VERSION;
    let encoding: u8 = 0;
    let encoding_and_data_type: u8 = (encoding << 6) as u8 | (data_type.to_u8() & 0x3F);

    [version, encoding_and_data_type]
}

pub fn unpack_game_package(bytes: [u8; GAME_PACKET_SIZE]) -> (u8, u8, DataType, u16) {
    let version: u8 = bytes[0];
    let encoding_and_type: u8 = bytes[1];
    let encoding: u8 = encoding_and_type >> 6;
    let data_type: DataType = DataType::from_u8(encoding_and_type & 0x3F);

    let data: u16 = ((bytes[GAME_PACKET_SIZE - 2] as u16) << 8) | (bytes[GAME_PACKET_SIZE - 1] as u16);
    (version, encoding, data_type, data)
}

pub fn create_game_package(data_type: DataType, content: u16) -> [u8; PACKET_INFO_SIZE + GAME_PACKET_SIZE] {
    let version: u8 = GAME_PACKET_VERSION;
    let encoding: u8 = 0;
    let encoding_and_data_type: u8 = (encoding << 6) as u8 | (data_type.to_u8() & 0x3F);
    let data: [u8; 2] = [(content >> 8) as u8, (content &0xFF) as u8];

    [version, encoding_and_data_type, data[0], data[1]]
}

// 128 64 32 16 8 4 2 1
//  0  0  0  0  0 0 1 1 -> 3 because it has 2 and 1 as '1'
//  0  1  0  0  0 0 0 0 -> 64 because 64 is '1' --> can interpret as 'a'
// send number 3 --> 0000 0011
// [1, 1, 0, 1] -> [0000 0001, 0000 0001, 0000 0000, 0000 0001] <--
//
// I got inspired by the primeagen, I have yet to learn by experience what I want.
//
//
//game data package
//┌---------------┬---------------┬---------------┬---------------┐
//|1 2 3 4 5 6 7 8|1 2 3 4 5 6 7 8|1 2 3 4 5 6 7 8|1 2 3 4 5 6 7 8|
//├---------------┼---┬-----------┼---------------┴---------------┤
//|   version     |en | data_type | data                          |
//└---------------┴---┴-----------┴-------------------------------┘
