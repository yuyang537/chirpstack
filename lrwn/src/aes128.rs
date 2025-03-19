use std::fmt;
use std::str::FromStr;

use anyhow::Result;
#[cfg(feature = "sqlite")]
use diesel::sqlite::Sqlite;
#[cfg(feature = "diesel")]
use diesel::{backend::Backend, deserialize, serialize, sql_types::Binary};
#[cfg(feature = "serde")]
use serde::{
    de::{self, Visitor},
    Deserialize, Deserializer, Serialize, Serializer,
};

#[cfg(feature = "klee_analysis")]
use std::os::raw::c_void;

use crate::Error;

#[derive(Copy, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "diesel", derive(AsExpression, FromSqlRow))]
#[cfg_attr(feature = "diesel", diesel(sql_type = diesel::sql_types::Binary))]
pub struct AES128Key([u8; 16]);

impl AES128Key {
    pub fn null() -> Self {
        AES128Key([0; 16])
    }

    pub fn from_slice(b: &[u8]) -> Result<Self, Error> {
        if b.len() != 16 {
            return Err(Error::Aes128Length);
        }

        let mut bb: [u8; 16] = [0; 16];
        bb.copy_from_slice(b);

        Ok(AES128Key(bb))
    }

    pub fn from_bytes(b: [u8; 16]) -> Self {
        AES128Key(b)
    }

    pub fn to_bytes(&self) -> [u8; 16] {
        self.0
    }

    pub fn to_vec(&self) -> Vec<u8> {
        self.0.to_vec()
    }
    
    #[cfg(feature = "klee_analysis")]
    pub fn encrypt(&self, data: &mut [u8; 16]) {
        // 在KLEE分析模式下实现一个简化版本的AES加密
        // 实际应用中，这里会调用真实的AES加密算法
        for i in 0..16 {
            data[i] ^= self.0[i];
        }
    }
    
    #[cfg(feature = "klee_analysis")]
    pub fn decrypt(&self, data: &mut [u8; 16]) {
        // 在KLEE分析模式下实现一个简化版本的AES解密
        // 由于XOR操作是可逆的，我们可以使用相同的操作
        for i in 0..16 {
            data[i] ^= self.0[i];
        }
    }
}

impl fmt::Display for AES128Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

impl fmt::Debug for AES128Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

impl FromStr for AES128Key {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut bytes: [u8; 16] = [0; 16];
        if !s.is_empty() {
            hex::decode_to_slice(s, &mut bytes)?;
        }
        Ok(AES128Key(bytes))
    }
}

#[cfg(feature = "serde")]
impl Serialize for AES128Key {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for AES128Key {
    fn deserialize<D>(deserialize: D) -> Result<AES128Key, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserialize.deserialize_str(Aes128KeyVisitor)
    }
}

#[cfg(feature = "serde")]
struct Aes128KeyVisitor;

#[cfg(feature = "serde")]
impl<'de> Visitor<'de> for Aes128KeyVisitor {
    type Value = AES128Key;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("A hex encoded AES key of 128 bit is expected")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        AES128Key::from_str(value).map_err(|e| E::custom(format!("{}", e)))
    }
}

#[cfg(feature = "diesel")]
impl<ST, DB> deserialize::FromSql<ST, DB> for AES128Key
where
    DB: Backend,
    *const [u8]: deserialize::FromSql<ST, DB>,
{
    fn from_sql(value: <DB as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        let bytes = <Vec<u8> as deserialize::FromSql<ST, DB>>::from_sql(value)?;
        if bytes.len() != 16 {
            return Err("AES128Key type expects exactly 16 bytes".into());
        }

        let mut b: [u8; 16] = [0; 16];
        b.copy_from_slice(&bytes);

        Ok(AES128Key(b))
    }
}

#[cfg(feature = "postgres")]
impl serialize::ToSql<Binary, diesel::pg::Pg> for AES128Key
where
    [u8]: serialize::ToSql<Binary, diesel::pg::Pg>,
{
    fn to_sql(&self, out: &mut serialize::Output<'_, '_, diesel::pg::Pg>) -> serialize::Result {
        <[u8] as serialize::ToSql<Binary, diesel::pg::Pg>>::to_sql(
            &self.to_bytes(),
            &mut out.reborrow(),
        )
    }
}

#[cfg(feature = "sqlite")]
impl serialize::ToSql<Binary, Sqlite> for AES128Key {
    fn to_sql<'b>(&'b self, out: &mut serialize::Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(Vec::from(self.to_bytes().as_slice()));
        Ok(serialize::IsNull::No)
    }
}

// KLEE分析相关代码
#[cfg(feature = "klee_analysis")]
pub mod klee_support {
    use super::*;
    use std::os::raw::{c_void, c_char};
    
    // 直接定义KLEE外部函数，而不使用klee-sys
    extern "C" {
        pub fn klee_make_symbolic(addr: *mut c_void, size: usize, name: *const c_char);
        pub fn klee_assume(condition: bool);
        pub fn klee_assert(condition: bool);
    }
    
    pub fn make_symbolic_key(name: &str) -> AES128Key {
        let mut key = [0u8; 16];
        unsafe {
            klee_make_symbolic(
                key.as_mut_ptr() as *mut c_void,
                16,
                format!("{}\0", name).as_ptr(),
            );
        }
        AES128Key(key)
    }
    
    pub fn make_symbolic_data(data: &mut [u8; 16], name: &str) {
        unsafe {
            klee_make_symbolic(
                data.as_mut_ptr() as *mut c_void,
                16,
                format!("{}\0", name).as_ptr(),
            );
        }
    }
    
    // FFI导出函数，用于KLEE驱动程序调用
    #[no_mangle]
    pub extern "C" fn aes128_encrypt(key_ptr: *const u8, data_ptr: *mut u8) {
        let mut key_bytes = [0u8; 16];
        let mut data = [0u8; 16];
        
        unsafe {
            std::ptr::copy_nonoverlapping(key_ptr, key_bytes.as_mut_ptr(), 16);
            std::ptr::copy_nonoverlapping(data_ptr, data.as_mut_ptr(), 16);
            
            let key = AES128Key::from_bytes(key_bytes);
            key.encrypt(&mut data);
            
            std::ptr::copy_nonoverlapping(data.as_ptr(), data_ptr, 16);
        }
    }
    
    #[no_mangle]
    pub extern "C" fn aes128_decrypt(key_ptr: *const u8, data_ptr: *mut u8) {
        let mut key_bytes = [0u8; 16];
        let mut data = [0u8; 16];
        
        unsafe {
            std::ptr::copy_nonoverlapping(key_ptr, key_bytes.as_mut_ptr(), 16);
            std::ptr::copy_nonoverlapping(data_ptr, data.as_mut_ptr(), 16);
            
            let key = AES128Key::from_bytes(key_bytes);
            key.decrypt(&mut data);
            
            std::ptr::copy_nonoverlapping(data.as_ptr(), data_ptr, 16);
        }
    }
    
    // 测试辅助函数
    #[no_mangle]
    pub extern "C" fn test_aes_roundtrip() -> bool {
        let key = make_symbolic_key("test_key");
        let mut original_data = [0u8; 16];
        make_symbolic_data(&mut original_data, "original_data");
        
        // 备份原始数据用于后续比较
        let mut data_copy = [0u8; 16];
        data_copy.copy_from_slice(&original_data);
        
        // 加密
        key.encrypt(&mut data_copy);
        
        // 解密
        key.decrypt(&mut data_copy);
        
        // 验证解密后的数据与原始数据相同
        for i in 0..16 {
            if data_copy[i] != original_data[i] {
                return false;
            }
        }
        
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_string() {
        let key = AES128Key::from_bytes([
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06,
            0x07, 0x08,
        ]);
        assert_eq!("01020304050607080102030405060708", key.to_string());
    }

    #[test]
    fn test_from_str() {
        let key = AES128Key::from_bytes([
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06,
            0x07, 0x08,
        ]);
        assert_eq!(
            key,
            AES128Key::from_str("01020304050607080102030405060708").unwrap()
        );
    }
    
    #[cfg(feature = "klee_analysis")]
    #[test]
    fn test_encrypt_decrypt() {
        let key = AES128Key::from_bytes([
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06,
            0x07, 0x08,
        ]);
        
        let mut data = [0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80, 0x90, 0xA0, 0xB0, 0xC0, 0xD0, 0xE0, 0xF0, 0x00];
        let original_data = data;
        
        // 加密
        key.encrypt(&mut data);
        
        // 确保数据发生了变化
        assert_ne!(data, original_data);
        
        // 解密
        key.decrypt(&mut data);
        
        // 确保解密后与原始数据相同
        assert_eq!(data, original_data);
    }
}
