use std::convert::TryFrom;

use minicbor::{
    decode,
    encode::{self, Write},
    Decode, Decoder, Encode, Encoder,
};

use crate::{
    refspec::{PatternStr, PatternString},
    Namespaced, Qualified, RefStr, RefString,
};

impl<'de: 'a, 'a> Decode<'de> for &'a RefStr {
    #[inline]
    fn decode(d: &mut Decoder<'de>) -> Result<Self, decode::Error> {
        d.str()
            .and_then(|s| Self::try_from(s).map_err(|e| decode::Error::Custom(Box::new(e))))
    }
}

impl Encode for &RefStr {
    #[inline]
    fn encode<W: Write>(&self, e: &mut Encoder<W>) -> Result<(), encode::Error<W::Error>> {
        e.str(self.as_str())?;
        Ok(())
    }
}

impl<'de> Decode<'de> for RefString {
    #[inline]
    fn decode(d: &mut Decoder<'de>) -> Result<Self, decode::Error> {
        Decode::decode(d).map(|s: &RefStr| s.to_owned())
    }
}

impl Encode for RefString {
    #[inline]
    fn encode<W: Write>(&self, e: &mut Encoder<W>) -> Result<(), encode::Error<W::Error>> {
        self.as_refstr().encode(e)
    }
}

impl<'de: 'a, 'a> Decode<'de> for &'a PatternStr {
    #[inline]
    fn decode(d: &mut Decoder<'de>) -> Result<Self, decode::Error> {
        d.str()
            .and_then(|s| Self::try_from(s).map_err(|e| decode::Error::Custom(Box::new(e))))
    }
}

impl Encode for &PatternStr {
    #[inline]
    fn encode<W: Write>(&self, e: &mut Encoder<W>) -> Result<(), encode::Error<W::Error>> {
        e.str(self.as_str())?;
        Ok(())
    }
}

impl<'de> Decode<'de> for PatternString {
    #[inline]
    fn decode(d: &mut Decoder<'de>) -> Result<Self, decode::Error> {
        Decode::decode(d).map(|s: &PatternStr| s.to_owned())
    }
}

impl Encode for PatternString {
    #[inline]
    fn encode<W: Write>(&self, e: &mut Encoder<W>) -> Result<(), encode::Error<W::Error>> {
        self.as_pattern_str().encode(e)
    }
}

impl<'de: 'a, 'a> Decode<'de> for Qualified<'a> {
    #[inline]
    fn decode(d: &mut Decoder<'de>) -> Result<Self, decode::Error> {
        Decode::decode(d).and_then(|s: &RefStr| {
            s.qualified()
                .ok_or(decode::Error::Message("not a qualified ref"))
        })
    }
}

impl Encode for Qualified<'_> {
    #[inline]
    fn encode<W: Write>(&self, e: &mut Encoder<W>) -> Result<(), encode::Error<W::Error>> {
        self.as_str().encode(e)
    }
}

impl<'de: 'a, 'a> Decode<'de> for Namespaced<'a> {
    #[inline]
    fn decode(d: &mut Decoder<'de>) -> Result<Self, decode::Error> {
        Decode::decode(d).and_then(|s: &RefStr| {
            s.to_namespaced()
                .ok_or(decode::Error::Message("not a namespaced ref"))
        })
    }
}

impl Encode for Namespaced<'_> {
    #[inline]
    fn encode<W: Write>(&self, e: &mut Encoder<W>) -> Result<(), encode::Error<W::Error>> {
        self.as_str().encode(e)
    }
}
