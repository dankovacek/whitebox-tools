use super::Ifd;
use whitebox_common::spatial_ref_system;
use whitebox_common::utils::{ByteOrderReader, Endianness};
use std::collections::HashMap;
use std::fmt;
use std::io::Cursor;
use std::mem::transmute;

macro_rules! hashmap {
    ($( $key: expr => $val: expr ),*) => {{
         let mut map = ::std::collections::HashMap::new();
         $( map.insert($key, $val); )*
         map
    }}
}

pub(super) struct GeoKeyEntry {
    pub tag: u16,
    pub location: u16,
    pub count: u16,
    pub value_offset: u16,
}

#[derive(Default, Clone, Debug)]
pub struct GeoKeys {
    geo_key_directory: Vec<u16>,
    geo_double_params: Vec<f64>,
    geo_ascii_params: String,
    tags: Vec<TiffTag>,
}

impl GeoKeys {
    pub fn add_key_directory(&mut self, data: &Vec<u8>, byte_order: Endianness) {
        // convert the binary data to an array of u16's
        let mut bor =
            ByteOrderReader::<Cursor<Vec<u8>>>::new(Cursor::new(data.clone()), byte_order);
        let mut i: usize = 0;
        while i < data.len() as usize {
            let k: u16 = bor.read_u16().unwrap();
            self.geo_key_directory.push(k);
            i += 2;
        }
    }

    pub fn add_double_params(&mut self, data: &Vec<u8>, byte_order: Endianness) {
        let mut i: usize = 0;
        let mut bor =
            ByteOrderReader::<Cursor<Vec<u8>>>::new(Cursor::new(data.clone()), byte_order);
        while i < data.len() as usize {
            let k: f64 = bor.read_f64().unwrap();
            i += 8;
            self.geo_double_params.push(k);
        }
    }

    pub fn add_ascii_params(&mut self, data: &Vec<u8>) {
        self.geo_ascii_params = String::from_utf8_lossy(&data[0..data.len()])
            .trim()
            .to_owned();
    }

    pub fn get_ifd_map(&self, byte_order: Endianness) -> HashMap<u16, Ifd> {
        if self.geo_key_directory.len() == 0 {
            panic!("Error reading geokeys");
        }
        let number_of_keys = self.geo_key_directory[3];

        let mut ifd_map: HashMap<u16, Ifd> = HashMap::new();
        // println!("Num geokeys: {}", number_of_keys);
        for i in 0..number_of_keys as usize {
            //println!("key number {}", i);
            let offset = 4 * (i + 1);
            let key_id = self.geo_key_directory[offset];

            let mut field_type: u16 = 0;
            let tiff_tag_location = self.geo_key_directory[offset + 1];
            let count = self.geo_key_directory[offset + 2];
            let value_offset = self.geo_key_directory[offset + 3];
            let mut data: Vec<u8> = vec![];
            if tiff_tag_location == 34737 {
                // ascii data
                field_type = 2;
                let value: &str =
                    &self.geo_ascii_params[value_offset as usize..(value_offset + count) as usize];
                let value2 = value.replace("|", "");
                data = value2.into_bytes();
            } else if tiff_tag_location == 34736 {
                // double (f64) data
                field_type = 12;
                let value =
                    &self.geo_double_params[value_offset as usize..(value_offset + count) as usize];
                for &v in value {
                    let byte_array = unsafe { transmute::<f64, [u8; 8]>(v) };
                    for i in 0..8 {
                        data.push(byte_array[i]);
                    }
                }
            } else if tiff_tag_location == 0 {
                // short (u16) data
                field_type = 3;
                let byte_array = unsafe { transmute::<u16, [u8; 2]>(value_offset) };
                for i in 0..2 {
                    data.push(byte_array[i]);
                }
            } else {
            }

            let ifd = Ifd::new(
                key_id,
                field_type,
                count as u64,
                value_offset as u64,
                data,
                byte_order,
            );
            ifd_map.insert(key_id, ifd.clone());
        }

        ifd_map
    }

    pub fn find_epsg_code(&self) -> u16 {
        if self.geo_key_directory.len() == 0 {
            return 0u16;
        }
        let keys = get_keys_map();
        let number_of_keys = self.geo_key_directory[3];

        let mut epsg_code = 0u16;

        for i in 0..number_of_keys as usize {
            let offset = 4 * (i + 1);
            let key_id = self.geo_key_directory[offset];
            let unknown_tag = TiffTag::new_unknown_tag();
            let key = match keys.get(&key_id) {
                Some(key) => key,
                None => &unknown_tag,
            };

            if key.code == 3072 || key.code == 2048 {
                epsg_code = self.geo_key_directory[offset + 3];
            } else if key.code == 2048 && epsg_code == 0u16 {
                epsg_code = self.geo_key_directory[offset + 3];
            }
        }

        epsg_code
    }

    pub fn interpret_geokeys(&self) -> String {
        if self.geo_key_directory.len() == 0 {
            return "GeoKeys have not been set.".to_string();
        }
        let keys = get_keys_map();
        let keyword_map = get_keyword_map();
        let mut s = "".to_string();
        // first read the geokey directory header
        let key_directory_version = self.geo_key_directory[0];
        let key_revision = self.geo_key_directory[1];
        let minor_revision = self.geo_key_directory[2];
        let number_of_keys = self.geo_key_directory[3];

        s = s + &format!(
            "GeoKey Info:
    Version: {}
    Key revision: {}.{}",
            key_directory_version, key_revision, minor_revision
        );

        for i in 0..number_of_keys as usize {
            let offset = 4 * (i + 1);
            if offset > (self.geo_key_directory.len() - 4) / 4 {
                break;
            }
            let key_id = self.geo_key_directory[offset];
            let unknown_tag = TiffTag::new_unknown_tag();
            let key = match keys.get(&key_id) {
                Some(key) => key,
                None => &unknown_tag, //&TiffTag::new_unknown_tag()
            };

            let tiff_tag_location = self.geo_key_directory[offset + 1];
            let count = self.geo_key_directory[offset + 2];
            let value_offset = self.geo_key_directory[offset + 3];
            if tiff_tag_location == 34737 {
                let value: &str =
                    &self.geo_ascii_params[value_offset as usize..(value_offset + count) as usize];
                let value2 = value.replace("|", "");
                s = s + &format!("\n{} (code={}, type=ASCII): {}", key.name, key.code, value2);
            } else if tiff_tag_location == 34736 {
                let value =
                    &self.geo_double_params[value_offset as usize..(value_offset + count) as usize];
                if count > 1 {
                    s = s + &format!(
                        "\n{} (code={}, type=Double, count={}): {:?}",
                        key.name, key.code, count, value
                    );
                } else {
                    s = s + &format!(
                        "\n{} (code={}, type=Double, count={}): {}",
                        key.name, key.code, count, value[0]
                    );
                }
            } else if tiff_tag_location == 0 {
                let key_code = key.code;
                let value: String;
                if keyword_map.contains_key(&key_code) {
                    match keyword_map.get(&key_code) {
                        Some(hm) => match hm.get(&value_offset) {
                            Some(v) => {
                                value = if key_code == 3072 || key_code == 2048 {
                                    format!(
                                        "{} ({})",
                                        v.to_string(),
                                        spatial_ref_system::esri_wkt_from_epsg(value_offset)
                                    )
                                } else {
                                    format!("{} ({})", v.to_string(), value_offset)
                                };
                            }
                            None => {
                                value = if key_code == 3072 || key_code == 2048 {
                                    spatial_ref_system::esri_wkt_from_epsg(value_offset)
                                } else {
                                    format!("Unrecognized value ({})", value_offset)
                                };
                            }
                        },
                        None => value = format!("Unrecognized value ({})", key_code),
                    }
                    s = s + &format!("\n{} (code={}): {}", key.name, key.code, value);
                } else {
                    s = s + &format!("\n{} (code={}): {}", key.name, key.code, value_offset);
                }
            } else {
                s = s + "Unknown tag";
            }
        }

        return s;
    }
}

pub fn get_keys_map() -> HashMap<u16, TiffTag> {
    let mut k = HashMap::new();
    k.insert(
        254u16,
        TiffTag {
            name: "NewSubFileType".to_string(),
            code: 254,
        },
    );
    k.insert(
        256u16,
        TiffTag {
            name: "ImageWidth".to_string(),
            code: 256,
        },
    );
    k.insert(
        257u16,
        TiffTag {
            name: "ImageLength".to_string(),
            code: 257,
        },
    );
    k.insert(
        258u16,
        TiffTag {
            name: "BitsPerSample".to_string(),
            code: 258,
        },
    );
    k.insert(
        259u16,
        TiffTag {
            name: "Compression".to_string(),
            code: 259,
        },
    );
    k.insert(
        262u16,
        TiffTag {
            name: "PhotometricInterpretation".to_string(),
            code: 262,
        },
    );
    k.insert(
        266u16,
        TiffTag {
            name: "FillOrder".to_string(),
            code: 266,
        },
    );
    k.insert(
        269u16,
        TiffTag {
            name: "DocumentName".to_string(),
            code: 269,
        },
    );
    k.insert(
        270u16,
        TiffTag {
            name: "ImageDescription".to_string(),
            code: 270,
        },
    );
    k.insert(
        271u16,
        TiffTag {
            name: "Make".to_string(),
            code: 271,
        },
    );
    k.insert(
        272u16,
        TiffTag {
            name: "Model".to_string(),
            code: 272,
        },
    );
    k.insert(
        273u16,
        TiffTag {
            name: "StripOffsets".to_string(),
            code: 273,
        },
    );
    k.insert(
        274u16,
        TiffTag {
            name: "Orientation".to_string(),
            code: 274,
        },
    );
    k.insert(
        277u16,
        TiffTag {
            name: "SamplesPerPixel".to_string(),
            code: 277,
        },
    );
    k.insert(
        278u16,
        TiffTag {
            name: "RowsPerStrip".to_string(),
            code: 278,
        },
    );
    k.insert(
        279u16,
        TiffTag {
            name: "StripByteCounts".to_string(),
            code: 279,
        },
    );
    k.insert(
        280u16,
        TiffTag {
            name: "MinSampleValue".to_string(),
            code: 280,
        },
    );
    k.insert(
        281u16,
        TiffTag {
            name: "MaxSampleValue".to_string(),
            code: 281,
        },
    );
    k.insert(
        282u16,
        TiffTag {
            name: "XResolution".to_string(),
            code: 282,
        },
    );
    k.insert(
        283u16,
        TiffTag {
            name: "YResolution".to_string(),
            code: 283,
        },
    );
    k.insert(
        284u16,
        TiffTag {
            name: "PlanarConfiguration".to_string(),
            code: 284,
        },
    );
    k.insert(
        296u16,
        TiffTag {
            name: "ResolutionUnit".to_string(),
            code: 296,
        },
    );
    k.insert(
        305u16,
        TiffTag {
            name: "Software".to_string(),
            code: 305,
        },
    );
    k.insert(
        306u16,
        TiffTag {
            name: "DateTime".to_string(),
            code: 306,
        },
    );
    k.insert(
        322u16,
        TiffTag {
            name: "TileWidth".to_string(),
            code: 322,
        },
    );
    k.insert(
        323u16,
        TiffTag {
            name: "TileLength".to_string(),
            code: 323,
        },
    );
    k.insert(
        324u16,
        TiffTag {
            name: "TileOffsets".to_string(),
            code: 324,
        },
    );
    k.insert(
        325u16,
        TiffTag {
            name: "TileByteCounts".to_string(),
            code: 325,
        },
    );
    k.insert(
        317u16,
        TiffTag {
            name: "Predictor".to_string(),
            code: 317,
        },
    );
    k.insert(
        320u16,
        TiffTag {
            name: "ColorMap".to_string(),
            code: 320,
        },
    );
    k.insert(
        338u16,
        TiffTag {
            name: "ExtraSamples".to_string(),
            code: 338,
        },
    );
    k.insert(
        339u16,
        TiffTag {
            name: "SampleFormat".to_string(),
            code: 339,
        },
    );
    k.insert(
        340u16,
        TiffTag {
            name: "SMinSampleValue".to_string(),
            code: 340,
        },
    );
    k.insert(
        341u16,
        TiffTag {
            name: "SMaxSampleValue".to_string(),
            code: 341,
        },
    );
    k.insert(
        347u16,
        TiffTag {
            name: "JPEGTables".to_string(),
            code: 347,
        },
    );
    k.insert(
        532u16,
        TiffTag {
            name: "ReferenceBlackWhite".to_string(),
            code: 532,
        },
    );

    k.insert(
        34735u16,
        TiffTag {
            name: "GeoKeyDirectoryTag".to_string(),
            code: 34735,
        },
    );
    k.insert(
        34736u16,
        TiffTag {
            name: "GeoDoubleParamsTag".to_string(),
            code: 34736,
        },
    );
    k.insert(
        34737u16,
        TiffTag {
            name: "GeoAsciiParamsTag".to_string(),
            code: 34737,
        },
    );
    k.insert(
        33550u16,
        TiffTag {
            name: "ModelPixelScaleTag".to_string(),
            code: 33550,
        },
    );
    k.insert(
        33922u16,
        TiffTag {
            name: "ModelTiepointTag".to_string(),
            code: 33922,
        },
    );
    k.insert(
        34264u16,
        TiffTag {
            name: "ModelTransformationTag".to_string(),
            code: 34264,
        },
    );
    k.insert(
        42112u16,
        TiffTag {
            name: "GDAL_METADATA".to_string(),
            code: 42112,
        },
    );
    k.insert(
        42113u16,
        TiffTag {
            name: "GDAL_NODATA".to_string(),
            code: 42113,
        },
    );

    k.insert(
        1024u16,
        TiffTag {
            name: "GTModelTypeGeoKey".to_string(),
            code: 1024,
        },
    );
    k.insert(
        1025u16,
        TiffTag {
            name: "GTRasterTypeGeoKey".to_string(),
            code: 1025,
        },
    );
    k.insert(
        1026u16,
        TiffTag {
            name: "GTCitationGeoKey".to_string(),
            code: 1026,
        },
    );
    k.insert(
        2048u16,
        TiffTag {
            name: "GeographicTypeGeoKey".to_string(),
            code: 2048,
        },
    );
    k.insert(
        2049u16,
        TiffTag {
            name: "GeogCitationGeoKey".to_string(),
            code: 2049,
        },
    );
    k.insert(
        2050u16,
        TiffTag {
            name: "GeogGeodeticDatumGeoKey".to_string(),
            code: 2050,
        },
    );
    k.insert(
        2051u16,
        TiffTag {
            name: "GeogPrimeMeridianGeoKey".to_string(),
            code: 2051,
        },
    );
    k.insert(
        2061u16,
        TiffTag {
            name: "GeogPrimeMeridianLongGeoKey".to_string(),
            code: 2061,
        },
    );
    k.insert(
        2052u16,
        TiffTag {
            name: "GeogLinearUnitsGeoKey".to_string(),
            code: 2052,
        },
    );
    k.insert(
        2053u16,
        TiffTag {
            name: "GeogLinearUnitSizeGeoKey".to_string(),
            code: 2053,
        },
    );
    k.insert(
        2054u16,
        TiffTag {
            name: "GeogAngularUnitsGeoKey".to_string(),
            code: 2054,
        },
    );
    k.insert(
        2055u16,
        TiffTag {
            name: "GeogAngularUnitSizeGeoKey".to_string(),
            code: 2055,
        },
    );
    k.insert(
        2056u16,
        TiffTag {
            name: "GeogEllipsoidGeoKey".to_string(),
            code: 2056,
        },
    );
    k.insert(
        2057u16,
        TiffTag {
            name: "GeogSemiMajorAxisGeoKey".to_string(),
            code: 2057,
        },
    );
    k.insert(
        2058u16,
        TiffTag {
            name: "GeogSemiMinorAxisGeoKey".to_string(),
            code: 2058,
        },
    );
    k.insert(
        2059u16,
        TiffTag {
            name: "GeogInvFlatteningGeoKey".to_string(),
            code: 2059,
        },
    );
    k.insert(
        2060u16,
        TiffTag {
            name: "GeogAzimuthUnitsGeoKey".to_string(),
            code: 2060,
        },
    );
    k.insert(
        3072u16,
        TiffTag {
            name: "ProjectedCSTypeGeoKey".to_string(),
            code: 3072,
        },
    );
    k.insert(
        3073u16,
        TiffTag {
            name: "PCSCitationGeoKey".to_string(),
            code: 3073,
        },
    );
    k.insert(
        3074u16,
        TiffTag {
            name: "ProjectionGeoKey".to_string(),
            code: 3074,
        },
    );
    k.insert(
        3075u16,
        TiffTag {
            name: "ProjCoordTransGeoKey".to_string(),
            code: 3075,
        },
    );
    k.insert(
        3076u16,
        TiffTag {
            name: "ProjLinearUnitsGeoKey".to_string(),
            code: 3076,
        },
    );
    k.insert(
        3077u16,
        TiffTag {
            name: "ProjLinearUnitSizeGeoKey".to_string(),
            code: 3077,
        },
    );
    k.insert(
        3078u16,
        TiffTag {
            name: "ProjStdParallel1GeoKey".to_string(),
            code: 3078,
        },
    );
    k.insert(
        3079u16,
        TiffTag {
            name: "ProjStdParallel2GeoKey".to_string(),
            code: 3079,
        },
    );
    k.insert(
        3080u16,
        TiffTag {
            name: "ProjNatOriginLongGeoKey".to_string(),
            code: 3080,
        },
    );
    k.insert(
        3081u16,
        TiffTag {
            name: "ProjNatOriginLatGeoKey".to_string(),
            code: 3081,
        },
    );
    k.insert(
        3082u16,
        TiffTag {
            name: "ProjFalseEastingGeoKey".to_string(),
            code: 3082,
        },
    );
    k.insert(
        3083u16,
        TiffTag {
            name: "ProjFalseNorthingGeoKey".to_string(),
            code: 3083,
        },
    );
    k.insert(
        3084u16,
        TiffTag {
            name: "ProjFalseOriginLongGeoKey".to_string(),
            code: 3084,
        },
    );
    k.insert(
        3085u16,
        TiffTag {
            name: "ProjFalseOriginLatGeoKey".to_string(),
            code: 3085,
        },
    );
    k.insert(
        3086u16,
        TiffTag {
            name: "ProjFalseOriginEastingGeoKey".to_string(),
            code: 3086,
        },
    );
    k.insert(
        3087u16,
        TiffTag {
            name: "ProjFalseOriginNorthingGeoKey".to_string(),
            code: 3087,
        },
    );
    k.insert(
        3088u16,
        TiffTag {
            name: "ProjCenterLongGeoKey".to_string(),
            code: 3088,
        },
    );
    k.insert(
        3089u16,
        TiffTag {
            name: "ProjCenterLatGeoKey".to_string(),
            code: 3089,
        },
    );
    k.insert(
        3090u16,
        TiffTag {
            name: "ProjCenterEastingGeoKey".to_string(),
            code: 3090,
        },
    );
    k.insert(
        3091u16,
        TiffTag {
            name: "ProjFalseOriginNorthingGeoKey".to_string(),
            code: 3091,
        },
    );
    k.insert(
        3092u16,
        TiffTag {
            name: "ProjScaleAtNatOriginGeoKey".to_string(),
            code: 3092,
        },
    );
    k.insert(
        3093u16,
        TiffTag {
            name: "ProjScaleAtCenterGeoKey".to_string(),
            code: 3093,
        },
    );
    k.insert(
        3094u16,
        TiffTag {
            name: "ProjAzimuthAngleGeoKey".to_string(),
            code: 3094,
        },
    );
    k.insert(
        3095u16,
        TiffTag {
            name: "ProjStraightVertPoleLongGeoKey".to_string(),
            code: 3095,
        },
    );
    k.insert(
        4096u16,
        TiffTag {
            name: "VerticalCSTypeGeoKey".to_string(),
            code: 4096,
        },
    );
    k.insert(
        4097u16,
        TiffTag {
            name: "VerticalCitationGeoKey".to_string(),
            code: 4097,
        },
    );
    k.insert(
        4098u16,
        TiffTag {
            name: "VerticalDatumGeoKey".to_string(),
            code: 4098,
        },
    );
    k.insert(
        4099u16,
        TiffTag {
            name: "VerticalUnitsGeoKey".to_string(),
            code: 4099,
        },
    );
    k.insert(
        50844u16,
        TiffTag {
            name: "RPCCoefficientTag".to_string(),
            code: 50844,
        },
    );
    k.insert(
        34377u16,
        TiffTag {
            name: "Photoshop".to_string(),
            code: 34377,
        },
    );

    k
}

pub fn get_keyword_map() -> HashMap<u16, HashMap<u16, &'static str>> {
    let mut kw = HashMap::new();

    let compression_map = hashmap![
        1u16 => "None",
        2u16 => "CCITT",
        3u16 => "G3",
        4u16 => "G4",
        5u16 => "LZW",
        6u16 => "JPEGOld",
        7u16 => "JPEG",
        8u16 => "Deflate",
        32773u16 => "PackBits",
        32946u16 => "DeflateOld"
    ];
    kw.insert(259u16, compression_map);

    let photometric_map = hashmap![
        0u16 => "WhiteIsZero",
        1u16 => "BlackIsZero",
        2u16 => "RGB",
        3u16 => "Paletted",
        4u16 => "TransMask",
        5u16 => "pCMYK",
        6u16 => "pYCbCr",
        7u16 => "pCIELab"
    ];
    kw.insert(262u16, photometric_map);

    let planar_configuration_map = hashmap![
        1u16=>"Contiguous",
        2u16=>"Separate"
    ];
    kw.insert(284u16, planar_configuration_map);

    let resolution_unit_map = hashmap![
        1u16=>"None",
        2u16=>"Dots per inch",
        3u16=>"Dots per centimeter"
    ];
    kw.insert(296u16, resolution_unit_map);

    let predictor_map = hashmap![
        1u16=>"None",
        2u16=>"Horizontal"
    ];
    kw.insert(317u16, predictor_map);

    let sample_format_map = hashmap![
        1u16=>"Unsigned integer data",
        2u16=>"Signed integer data",
        3u16=>"Floating point data",
        4u16=>"Undefined data format"
    ];
    kw.insert(339u16, sample_format_map);

    let model_type_map = hashmap![
        1u16=>"ModelTypeProjected",
        2u16=>"ModelTypeGeographic",
        3u16=>"ModelTypeGeocentric"
    ];
    kw.insert(1024u16, model_type_map);

    let raster_type_map = hashmap![
        1u16=>"RasterPixelIsArea",
        2u16=>"RasterPixelIsPoint"
    ];
    kw.insert(1025u16, raster_type_map);

    let geographic_type_map = hashmap![
        4201=>"GCS_Adindan",
        4202=>"GCS_AGD66",
        4203=>"GCS_AGD84",
        4204=>"GCS_Ain_el_Abd",
        4205=>"GCS_Afgooye",
        4206=>"GCS_Agadez",
        4207=>"GCS_Lisbon",
        4208=>"GCS_Aratu",
        4209=>"GCS_Arc_1950",
        4210=>"GCS_Arc_1960",
        4211=>"GCS_Batavia",
        4212=>"GCS_Barbados",
        4213=>"GCS_Beduaram",
        4214=>"GCS_Beijing_1954",
        4215=>"GCS_Belge_1950",
        4216=>"GCS_Bermuda_1957",
        4217=>"GCS_Bern_1898",
        4218=>"GCS_Bogota",
        4219=>"GCS_Bukit_Rimpah",
        4220=>"GCS_Camacupa",
        4221=>"GCS_Campo_Inchauspe",
        4222=>"GCS_Cape",
        4223=>"GCS_Carthage",
        4224=>"GCS_Chua",
        4225=>"GCS_Corrego_Alegre",
        4226=>"GCS_Cote_d_Ivoire",
        4227=>"GCS_Deir_ez_Zor",
        4228=>"GCS_Douala",
        4229=>"GCS_Egypt_1907",
        4230=>"GCS_ED50",
        4231=>"GCS_ED87",
        4232=>"GCS_Fahud",
        4233=>"GCS_Gandajika_1970",
        4234=>"GCS_Garoua",
        4235=>"GCS_Guyane_Francaise",
        4236=>"GCS_Hu_Tzu_Shan",
        4237=>"GCS_HD72",
        4238=>"GCS_ID74",
        4239=>"GCS_Indian_1954",
        4240=>"GCS_Indian_1975",
        4241=>"GCS_Jamaica_1875",
        4242=>"GCS_JAD69",
        4243=>"GCS_Kalianpur",
        4244=>"GCS_Kandawala",
        4245=>"GCS_Kertau",
        4246=>"GCS_KOC",
        4247=>"GCS_La_Canoa",
        4248=>"GCS_PSAD56",
        4249=>"GCS_Lake",
        4250=>"GCS_Leigon",
        4251=>"GCS_Liberia_1964",
        4252=>"GCS_Lome",
        4253=>"GCS_Luzon_1911",
        4254=>"GCS_Hito_XVIII_1963",
        4255=>"GCS_Herat_North",
        4256=>"GCS_Mahe_1971",
        4257=>"GCS_Makassar",
        4258=>"GCS_EUREF89",
        4259=>"GCS_Malongo_1987",
        4260=>"GCS_Manoca",
        4261=>"GCS_Merchich",
        4262=>"GCS_Massawa",
        4263=>"GCS_Minna",
        4264=>"GCS_Mhast",
        4265=>"GCS_Monte_Mario",
        4266=>"GCS_M_poraloko",
        4267=>"GCS_NAD27",
        4268=>"GCS_NAD_Michigan",
        4269=>"GCS_NAD83",
        4270=>"GCS_Nahrwan_1967",
        4271=>"GCS_Naparima_1972",
        4272=>"GCS_GD49",
        4273=>"GCS_NGO_1948",
        4274=>"GCS_Datum_73",
        4275=>"GCS_NTF",
        4276=>"GCS_NSWC_9Z_2",
        4277=>"GCS_OSGB_1936",
        4278=>"GCS_OSGB70",
        4279=>"GCS_OS_SN80",
        4280=>"GCS_Padang",
        4281=>"GCS_Palestine_1923",
        4282=>"GCS_Pointe_Noire",
        4283=>"GCS_GDA94",
        4284=>"GCS_Pulkovo_1942",
        4285=>"GCS_Qatar",
        4286=>"GCS_Qatar_1948",
        4287=>"GCS_Qornoq",
        4288=>"GCS_Loma_Quintana",
        4289=>"GCS_Amersfoort",
        4290=>"GCS_RT38",
        4291=>"GCS_SAD69",
        4292=>"GCS_Sapper_Hill_1943",
        4293=>"GCS_Schwarzeck",
        4294=>"GCS_Segora",
        4295=>"GCS_Serindung",
        4296=>"GCS_Sudan",
        4297=>"GCS_Tananarive",
        4298=>"GCS_Timbalai_1948",
        4299=>"GCS_TM65",
        4300=>"GCS_TM75",
        4301=>"GCS_Tokyo",
        4302=>"GCS_Trinidad_1903",
        4303=>"GCS_TC_1948",
        4304=>"GCS_Voirol_1875",
        4305=>"GCS_Voirol_Unifie",
        4306=>"GCS_Bern_1938",
        4307=>"GCS_Nord_Sahara_1959",
        4308=>"GCS_Stockholm_1938",
        4309=>"GCS_Yacare",
        4310=>"GCS_Yoff",
        4311=>"GCS_Zanderij",
        4312=>"GCS_MGI",
        4313=>"GCS_Belge_1972",
        4314=>"GCS_DHDN",
        4315=>"GCS_Conakry_1905",
        4322=>"GCS_WGS_72",
        4324=>"GCS_WGS_72BE",
        4326=>"GCS_WGS_84",
        4801=>"GCS_Bern_1898_Bern",
        4802=>"GCS_Bogota_Bogota",
        4803=>"GCS_Lisbon_Lisbon",
        4804=>"GCS_Makassar_Jakarta",
        4805=>"GCS_MGI_Ferro",
        4806=>"GCS_Monte_Mario_Rome",
        4807=>"GCS_NTF_Paris",
        4808=>"GCS_Padang_Jakarta",
        4809=>"GCS_Belge_1950_Brussels",
        4810=>"GCS_Tananarive_Paris",
        4811=>"GCS_Voirol_1875_Paris",
        4812=>"GCS_Voirol_Unifie_Paris",
        4813=>"GCS_Batavia_Jakarta",
        4901=>"GCS_ATF_Paris",
        4902=>"GCS_NDG_Paris",
        4001=>"GCSE_Airy1830",
        4002=>"GCSE_AiryModified1849",
        4003=>"GCSE_AustralianNationalSpheroid",
        4004=>"GCSE_Bessel1841",
        4005=>"GCSE_BesselModified",
        4006=>"GCSE_BesselNamibia",
        4007=>"GCSE_Clarke1858",
        4008=>"GCSE_Clarke1866",
        4009=>"GCSE_Clarke1866Michigan",
        4010=>"GCSE_Clarke1880_Benoit",
        4011=>"GCSE_Clarke1880_IGN",
        4012=>"GCSE_Clarke1880_RGS",
        4013=>"GCSE_Clarke1880_Arc",
        4014=>"GCSE_Clarke1880_SGA1922",
        4015=>"GCSE_Everest1830_1937Adjustment",
        4016=>"GCSE_Everest1830_1967Definition",
        4017=>"GCSE_Everest1830_1975Definition",
        4018=>"GCSE_Everest1830Modified",
        4019=>"GCSE_GRS1980",
        4020=>"GCSE_Helmert1906",
        4021=>"GCSE_IndonesianNationalSpheroid",
        4022=>"GCSE_International1924",
        4023=>"GCSE_International1967",
        4024=>"GCSE_Krassowsky1940",
        4025=>"GCSE_NWL9D",
        4026=>"GCSE_NWL10D",
        4027=>"GCSE_Plessis1817",
        4028=>"GCSE_Struve1860",
        4029=>"GCSE_WarOffice",
        4030=>"GCSE_WGS84",
        4031=>"GCSE_GEM10C",
        4032=>"GCSE_OSU86F",
        4033=>"GCSE_OSU91A",
        4034=>"GCSE_Clarke1880",
        4035=>"GCSE_Sphere"
    ];
    kw.insert(2048u16, geographic_type_map);

    let geodetic_datum_map = hashmap![
        6201u16=>"Datum_Adindan",
        6202=>"Datum_Australian_Geodetic_Datum_1966",
        6203=>"Datum_Australian_Geodetic_Datum_1984",
        6204=>"Datum_Ain_el_Abd_1970",
        6205=>"Datum_Afgooye",
        6206=>"Datum_Agadez",
        6207=>"Datum_Lisbon",
        6208=>"Datum_Aratu",
        6209=>"Datum_Arc_1950",
        6210=>"Datum_Arc_1960",
        6211=>"Datum_Batavia",
        6212=>"Datum_Barbados",
        6213=>"Datum_Beduaram",
        6214=>"Datum_Beijing_1954",
        6215=>"Datum_Reseau_National_Belge_1950",
        6216=>"Datum_Bermuda_1957",
        6217=>"Datum_Bern_1898",
        6218=>"Datum_Bogota",
        6219=>"Datum_Bukit_Rimpah",
        6220=>"Datum_Camacupa",
        6221=>"Datum_Campo_Inchauspe",
        6222=>"Datum_Cape",
        6223=>"Datum_Carthage",
        6224=>"Datum_Chua",
        6225=>"Datum_Corrego_Alegre",
        6226=>"Datum_Cote_d_Ivoire",
        6227=>"Datum_Deir_ez_Zor",
        6228=>"Datum_Douala",
        6229=>"Datum_Egypt_1907",
        6230=>"Datum_European_Datum_1950",
        6231=>"Datum_European_Datum_1987",
        6232=>"Datum_Fahud",
        6233=>"Datum_Gandajika_1970",
        6234=>"Datum_Garoua",
        6235=>"Datum_Guyane_Francaise",
        6236=>"Datum_Hu_Tzu_Shan",
        6237=>"Datum_Hungarian_Datum_1972",
        6238=>"Datum_Indonesian_Datum_1974",
        6239=>"Datum_Indian_1954",
        6240=>"Datum_Indian_1975",
        6241=>"Datum_Jamaica_1875",
        6242=>"Datum_Jamaica_1969",
        6243=>"Datum_Kalianpur",
        6244=>"Datum_Kandawala",
        6245=>"Datum_Kertau",
        6246=>"Datum_Kuwait_Oil_Company",
        6247=>"Datum_La_Canoa",
        6248=>"Datum_Provisional_S_American_Datum_1956",
        6249=>"Datum_Lake",
        6250=>"Datum_Leigon",
        6251=>"Datum_Liberia_1964",
        6252=>"Datum_Lome",
        6253=>"Datum_Luzon_1911",
        6254=>"Datum_Hito_XVIII_1963",
        6255=>"Datum_Herat_North",
        6256=>"Datum_Mahe_1971",
        6257=>"Datum_Makassar",
        6258=>"Datum_European_Reference_System_1989",
        6259=>"Datum_Malongo_1987",
        6260=>"Datum_Manoca",
        6261=>"Datum_Merchich",
        6262=>"Datum_Massawa",
        6263=>"Datum_Minna",
        6264=>"Datum_Mhast",
        6265=>"Datum_Monte_Mario",
        6266=>"Datum_M_poraloko",
        6267=>"Datum_North_American_Datum_1927",
        6268=>"Datum_NAD_Michigan",
        6269=>"Datum_North_American_Datum_1983",
        6270=>"Datum_Nahrwan_1967",
        6271=>"Datum_Naparima_1972",
        6272=>"Datum_New_Zealand_Geodetic_Datum_1949",
        6273=>"Datum_NGO_1948",
        6274=>"Datum_Datum_73",
        6275=>"Datum_Nouvelle_Triangulation_Francaise",
        6276=>"Datum_NSWC_9Z_2",
        6277=>"Datum_OSGB_1936",
        6278=>"Datum_OSGB_1970_SN",
        6279=>"Datum_OS_SN_1980",
        6280=>"Datum_Padang_1884",
        6281=>"Datum_Palestine_1923",
        6282=>"Datum_Pointe_Noire",
        6283=>"Datum_Geocentric_Datum_of_Australia_1994",
        6284=>"Datum_Pulkovo_1942",
        6285=>"Datum_Qatar",
        6286=>"Datum_Qatar_1948",
        6287=>"Datum_Qornoq",
        6288=>"Datum_Loma_Quintana",
        6289=>"Datum_Amersfoort",
        6290=>"Datum_RT38",
        6291=>"Datum_South_American_Datum_1969",
        6292=>"Datum_Sapper_Hill_1943",
        6293=>"Datum_Schwarzeck",
        6294=>"Datum_Segora",
        6295=>"Datum_Serindung",
        6296=>"Datum_Sudan",
        6297=>"Datum_Tananarive_1925",
        6298=>"Datum_Timbalai_1948",
        6299=>"Datum_TM65",
        6300=>"Datum_TM75",
        6301=>"Datum_Tokyo",
        6302=>"Datum_Trinidad_1903",
        6303=>"Datum_Trucial_Coast_1948",
        6304=>"Datum_Voirol_1875",
        6305=>"Datum_Voirol_Unifie_1960",
        6306=>"Datum_Bern_1938",
        6307=>"Datum_Nord_Sahara_1959",
        6308=>"Datum_Stockholm_1938",
        6309=>"Datum_Yacare",
        6310=>"Datum_Yoff",
        6311=>"Datum_Zanderij",
        6312=>"Datum_Militar_Geographische_Institut",
        6313=>"Datum_Reseau_National_Belge_1972",
        6314=>"Datum_Deutsche_Hauptdreiecksnetz",
        6315=>"Datum_Conakry_1905",
        6322=>"Datum_WGS72",
        6324=>"Datum_WGS72_Transit_Broadcast_Ephemeris",
        6326=>"Datum_WGS84",
        6901=>"Datum_Ancienne_Triangulation_Francaise",
        6902=>"Datum_Nord_de_Guerre",
        6001=>"DatumE_Airy1830",
        6002=>"DatumE_AiryModified1849",
        6003=>"DatumE_AustralianNationalSpheroid",
        6004=>"DatumE_Bessel1841",
        6005=>"DatumE_BesselModified",
        6006=>"DatumE_BesselNamibia",
        6007=>"DatumE_Clarke1858",
        6008=>"DatumE_Clarke1866",
        6009=>"DatumE_Clarke1866Michigan",
        6010=>"DatumE_Clarke1880_Benoit",
        6011=>"DatumE_Clarke1880_IGN",
        6012=>"DatumE_Clarke1880_RGS",
        6013=>"DatumE_Clarke1880_Arc",
        6014=>"DatumE_Clarke1880_SGA1922",
        6015=>"DatumE_Everest1830_1937Adjustment",
        6016=>"DatumE_Everest1830_1967Definition",
        6017=>"DatumE_Everest1830_1975Definition",
        6018=>"DatumE_Everest1830Modified",
        6019=>"DatumE_GRS1980",
        6020=>"DatumE_Helmert1906",
        6021=>"DatumE_IndonesianNationalSpheroid",
        6022=>"DatumE_International1924",
        6023=>"DatumE_International1967",
        6024=>"DatumE_Krassowsky1960",
        6025=>"DatumE_NWL9D",
        6026=>"DatumE_NWL10D",
        6027=>"DatumE_Plessis1817",
        6028=>"DatumE_Struve1860",
        6029=>"DatumE_WarOffice",
        6030=>"DatumE_WGS84",
        6031=>"DatumE_GEM10C",
        6032=>"DatumE_OSU86F",
        6033=>"DatumE_OSU91A",
        6034=>"DatumE_Clarke1880",
        6035=>"DatumE_Sphere"
    ];
    kw.insert(2050u16, geodetic_datum_map);

    let geog_prime_meridian_map = hashmap![
        8901=>"PM_Greenwich",
        8902=>"PM_Lisbon",
        8903=>"PM_Paris",
        8904=>"PM_Bogota",
        8905=>"PM_Madrid",
        8906=>"PM_Rome",
        8907=>"PM_Bern",
        8908=>"PM_Jakarta",
        8909=>"PM_Ferro",
        8910=>"PM_Brussels",
        8911=>"PM_Stockholm"
    ];
    kw.insert(2051u16, geog_prime_meridian_map);

    let geog_angular_units_map = hashmap![
        9101=>"Angular_Radian",
        9102=>"Angular_Degree",
        9103=>"Angular_Arc_Minute",
        9104=>"Angular_Arc_Second",
        9105=>"Angular_Grad",
        9106=>"Angular_Gon",
        9107=>"Angular_DMS",
        9108=>"Angular_DMS_Hemisphere"
    ];
    kw.insert(2054u16, geog_angular_units_map);

    let ellipsoid_map = hashmap![
        7001u16=>"Ellipse_Airy_1830",
        7002=>"Ellipse_Airy_Modified_1849",
        7003=>"Ellipse_Australian_National_Spheroid",
        7004=>"Ellipse_Bessel_1841",
        7005=>"Ellipse_Bessel_Modified",
        7006=>"Ellipse_Bessel_Namibia",
        7007=>"Ellipse_Clarke_1858",
        7008=>"Ellipse_Clarke_1866",
        7009=>"Ellipse_Clarke_1866_Michigan",
        7010=>"Ellipse_Clarke_1880_Benoit",
        7011=>"Ellipse_Clarke_1880_IGN",
        7012=>"Ellipse_Clarke_1880_RGS",
        7013=>"Ellipse_Clarke_1880_Arc",
        7014=>"Ellipse_Clarke_1880_SGA_1922",
        7015=>"Ellipse_Everest_1830_1937_Adjustment",
        7016=>"Ellipse_Everest_1830_1967_Definition",
        7017=>"Ellipse_Everest_1830_1975_Definition",
        7018=>"Ellipse_Everest_1830_Modified",
        7019=>"Ellipse_GRS_1980",
        7020=>"Ellipse_Helmert_1906",
        7021=>"Ellipse_Indonesian_National_Spheroid",
        7022=>"Ellipse_International_1924",
        7023=>"Ellipse_International_1967",
        7024=>"Ellipse_Krassowsky_1940",
        7025=>"Ellipse_NWL_9D",
        7026=>"Ellipse_NWL_10D",
        7027=>"Ellipse_Plessis_1817",
        7028=>"Ellipse_Struve_1860",
        7029=>"Ellipse_War_Office",
        7030=>"Ellipse_WGS_84",
        7031=>"Ellipse_GEM_10C",
        7032=>"Ellipse_OSU86F",
        7033=>"Ellipse_OSU91A",
        7034=>"Ellipse_Clarke_1880",
        7035=>"Ellipse_Sphere"
    ];
    kw.insert(2056u16, ellipsoid_map);

    let projected_cs_type_map = hashmap![
        20137=>"PCS_Adindan_UTM_zone_37N",
        20138=>"PCS_Adindan_UTM_zone_38N",
        20248=>"PCS_AGD66_AMG_zone_48",
        20249=>"PCS_AGD66_AMG_zone_49",
        20250=>"PCS_AGD66_AMG_zone_50",
        20251=>"PCS_AGD66_AMG_zone_51",
        20252=>"PCS_AGD66_AMG_zone_52",
        20253=>"PCS_AGD66_AMG_zone_53",
        20254=>"PCS_AGD66_AMG_zone_54",
        20255=>"PCS_AGD66_AMG_zone_55",
        20256=>"PCS_AGD66_AMG_zone_56",
        20257=>"PCS_AGD66_AMG_zone_57",
        20258=>"PCS_AGD66_AMG_zone_58",
        20348=>"PCS_AGD84_AMG_zone_48",
        20349=>"PCS_AGD84_AMG_zone_49",
        20350=>"PCS_AGD84_AMG_zone_50",
        20351=>"PCS_AGD84_AMG_zone_51",
        20352=>"PCS_AGD84_AMG_zone_52",
        20353=>"PCS_AGD84_AMG_zone_53",
        20354=>"PCS_AGD84_AMG_zone_54",
        20355=>"PCS_AGD84_AMG_zone_55",
        20356=>"PCS_AGD84_AMG_zone_56",
        20357=>"PCS_AGD84_AMG_zone_57",
        20358=>"PCS_AGD84_AMG_zone_58",
        20437=>"PCS_Ain_el_Abd_UTM_zone_37N",
        20438=>"PCS_Ain_el_Abd_UTM_zone_38N",
        20439=>"PCS_Ain_el_Abd_UTM_zone_39N",
        20499=>"PCS_Ain_el_Abd_Bahrain_Grid",
        20538=>"PCS_Afgooye_UTM_zone_38N",
        20539=>"PCS_Afgooye_UTM_zone_39N",
        20700=>"PCS_Lisbon_Portugese_Grid",
        20822=>"PCS_Aratu_UTM_zone_22S",
        20823=>"PCS_Aratu_UTM_zone_23S",
        20824=>"PCS_Aratu_UTM_zone_24S",
        20973=>"PCS_Arc_1950_Lo13",
        20975=>"PCS_Arc_1950_Lo15",
        20977=>"PCS_Arc_1950_Lo17",
        20979=>"PCS_Arc_1950_Lo19",
        20981=>"PCS_Arc_1950_Lo21",
        20983=>"PCS_Arc_1950_Lo23",
        20985=>"PCS_Arc_1950_Lo25",
        20987=>"PCS_Arc_1950_Lo27",
        20989=>"PCS_Arc_1950_Lo29",
        20991=>"PCS_Arc_1950_Lo31",
        20993=>"PCS_Arc_1950_Lo33",
        20995=>"PCS_Arc_1950_Lo35",
        21100=>"PCS_Batavia_NEIEZ",
        21148=>"PCS_Batavia_UTM_zone_48S",
        21149=>"PCS_Batavia_UTM_zone_49S",
        21150=>"PCS_Batavia_UTM_zone_50S",
        21413=>"PCS_Beijing_Gauss_zone_13",
        21414=>"PCS_Beijing_Gauss_zone_14",
        21415=>"PCS_Beijing_Gauss_zone_15",
        21416=>"PCS_Beijing_Gauss_zone_16",
        21417=>"PCS_Beijing_Gauss_zone_17",
        21418=>"PCS_Beijing_Gauss_zone_18",
        21419=>"PCS_Beijing_Gauss_zone_19",
        21420=>"PCS_Beijing_Gauss_zone_20",
        21421=>"PCS_Beijing_Gauss_zone_21",
        21422=>"PCS_Beijing_Gauss_zone_22",
        21423=>"PCS_Beijing_Gauss_zone_23",
        21473=>"PCS_Beijing_Gauss_13N",
        21474=>"PCS_Beijing_Gauss_14N",
        21475=>"PCS_Beijing_Gauss_15N",
        21476=>"PCS_Beijing_Gauss_16N",
        21477=>"PCS_Beijing_Gauss_17N",
        21478=>"PCS_Beijing_Gauss_18N",
        21479=>"PCS_Beijing_Gauss_19N",
        21480=>"PCS_Beijing_Gauss_20N",
        21481=>"PCS_Beijing_Gauss_21N",
        21482=>"PCS_Beijing_Gauss_22N",
        21483=>"PCS_Beijing_Gauss_23N",
        21500=>"PCS_Belge_Lambert_50",
        21790=>"PCS_Bern_1898_Swiss_Old",
        21817=>"PCS_Bogota_UTM_zone_17N",
        21818=>"PCS_Bogota_UTM_zone_18N",
        21891=>"PCS_Bogota_Colombia_3W",
        21892=>"PCS_Bogota_Colombia_Bogota",
        21893=>"PCS_Bogota_Colombia_3E",
        21894=>"PCS_Bogota_Colombia_6E",
        22032=>"PCS_Camacupa_UTM_32S",
        22033=>"PCS_Camacupa_UTM_33S",
        22191=>"PCS_C_Inchauspe_Argentina_1",
        22192=>"PCS_C_Inchauspe_Argentina_2",
        22193=>"PCS_C_Inchauspe_Argentina_3",
        22194=>"PCS_C_Inchauspe_Argentina_4",
        22195=>"PCS_C_Inchauspe_Argentina_5",
        22196=>"PCS_C_Inchauspe_Argentina_6",
        22197=>"PCS_C_Inchauspe_Argentina_7",
        22332=>"PCS_Carthage_UTM_zone_32N",
        22391=>"PCS_Carthage_Nord_Tunisie",
        22392=>"PCS_Carthage_Sud_Tunisie",
        22523=>"PCS_Corrego_Alegre_UTM_23S",
        22524=>"PCS_Corrego_Alegre_UTM_24S",
        22832=>"PCS_Douala_UTM_zone_32N",
        22992=>"PCS_Egypt_1907_Red_Belt",
        22993=>"PCS_Egypt_1907_Purple_Belt",
        22994=>"PCS_Egypt_1907_Ext_Purple",
        23028=>"PCS_ED50_UTM_zone_28N",
        23029=>"PCS_ED50_UTM_zone_29N",
        23030=>"PCS_ED50_UTM_zone_30N",
        23031=>"PCS_ED50_UTM_zone_31N",
        23032=>"PCS_ED50_UTM_zone_32N",
        23033=>"PCS_ED50_UTM_zone_33N",
        23034=>"PCS_ED50_UTM_zone_34N",
        23035=>"PCS_ED50_UTM_zone_35N",
        23036=>"PCS_ED50_UTM_zone_36N",
        23037=>"PCS_ED50_UTM_zone_37N",
        23038=>"PCS_ED50_UTM_zone_38N",
        23239=>"PCS_Fahud_UTM_zone_39N",
        23240=>"PCS_Fahud_UTM_zone_40N",
        23433=>"PCS_Garoua_UTM_zone_33N",
        23846=>"PCS_ID74_UTM_zone_46N",
        23847=>"PCS_ID74_UTM_zone_47N",
        23848=>"PCS_ID74_UTM_zone_48N",
        23849=>"PCS_ID74_UTM_zone_49N",
        23850=>"PCS_ID74_UTM_zone_50N",
        23851=>"PCS_ID74_UTM_zone_51N",
        23852=>"PCS_ID74_UTM_zone_52N",
        23853=>"PCS_ID74_UTM_zone_53N",
        23886=>"PCS_ID74_UTM_zone_46S",
        23887=>"PCS_ID74_UTM_zone_47S",
        23888=>"PCS_ID74_UTM_zone_48S",
        23889=>"PCS_ID74_UTM_zone_49S",
        23890=>"PCS_ID74_UTM_zone_50S",
        23891=>"PCS_ID74_UTM_zone_51S",
        23892=>"PCS_ID74_UTM_zone_52S",
        23893=>"PCS_ID74_UTM_zone_53S",
        23894=>"PCS_ID74_UTM_zone_54S",
        23947=>"PCS_Indian_1954_UTM_47N",
        23948=>"PCS_Indian_1954_UTM_48N",
        24047=>"PCS_Indian_1975_UTM_47N",
        24048=>"PCS_Indian_1975_UTM_48N",
        24100=>"PCS_Jamaica_1875_Old_Grid",
        24200=>"PCS_JAD69_Jamaica_Grid",
        24370=>"PCS_Kalianpur_India_0",
        24371=>"PCS_Kalianpur_India_I",
        24372=>"PCS_Kalianpur_India_IIa",
        24373=>"PCS_Kalianpur_India_IIIa",
        24374=>"PCS_Kalianpur_India_IVa",
        24382=>"PCS_Kalianpur_India_IIb",
        24383=>"PCS_Kalianpur_India_IIIb",
        24384=>"PCS_Kalianpur_India_IVb",
        24500=>"PCS_Kertau_Singapore_Grid",
        24547=>"PCS_Kertau_UTM_zone_47N",
        24548=>"PCS_Kertau_UTM_zone_48N",
        24720=>"PCS_La_Canoa_UTM_zone_20N",
        24721=>"PCS_La_Canoa_UTM_zone_21N",
        24818=>"PCS_PSAD56_UTM_zone_18N",
        24819=>"PCS_PSAD56_UTM_zone_19N",
        24820=>"PCS_PSAD56_UTM_zone_20N",
        24821=>"PCS_PSAD56_UTM_zone_21N",
        24877=>"PCS_PSAD56_UTM_zone_17S",
        24878=>"PCS_PSAD56_UTM_zone_18S",
        24879=>"PCS_PSAD56_UTM_zone_19S",
        24880=>"PCS_PSAD56_UTM_zone_20S",
        24891=>"PCS_PSAD56_Peru_west_zone",
        24892=>"PCS_PSAD56_Peru_central",
        24893=>"PCS_PSAD56_Peru_east_zone",
        25000=>"PCS_Leigon_Ghana_Grid",
        25231=>"PCS_Lome_UTM_zone_31N",
        25391=>"PCS_Luzon_Philippines_I",
        25392=>"PCS_Luzon_Philippines_II",
        25393=>"PCS_Luzon_Philippines_III",
        25394=>"PCS_Luzon_Philippines_IV",
        25395=>"PCS_Luzon_Philippines_V",
        25700=>"PCS_Makassar_NEIEZ",
        25932=>"PCS_Malongo_1987_UTM_32S",
        26191=>"PCS_Merchich_Nord_Maroc",
        26192=>"PCS_Merchich_Sud_Maroc",
        26193=>"PCS_Merchich_Sahara",
        26237=>"PCS_Massawa_UTM_zone_37N",
        26331=>"PCS_Minna_UTM_zone_31N",
        26332=>"PCS_Minna_UTM_zone_32N",
        26391=>"PCS_Minna_Nigeria_West",
        26392=>"PCS_Minna_Nigeria_Mid_Belt",
        26393=>"PCS_Minna_Nigeria_East",
        26432=>"PCS_Mhast_UTM_zone_32S",
        26591=>"PCS_Monte_Mario_Italy_1",
        26592=>"PCS_Monte_Mario_Italy_2",
        26632=>"PCS_M_poraloko_UTM_32N",
        26692=>"PCS_M_poraloko_UTM_32S",
        26703=>"PCS_NAD27_UTM_zone_3N",
        26704=>"PCS_NAD27_UTM_zone_4N",
        26705=>"PCS_NAD27_UTM_zone_5N",
        26706=>"PCS_NAD27_UTM_zone_6N",
        26707=>"PCS_NAD27_UTM_zone_7N",
        26708=>"PCS_NAD27_UTM_zone_8N",
        26709=>"PCS_NAD27_UTM_zone_9N",
        26710=>"PCS_NAD27_UTM_zone_10N",
        26711=>"PCS_NAD27_UTM_zone_11N",
        26712=>"PCS_NAD27_UTM_zone_12N",
        26713=>"PCS_NAD27_UTM_zone_13N",
        26714=>"PCS_NAD27_UTM_zone_14N",
        26715=>"PCS_NAD27_UTM_zone_15N",
        26716=>"PCS_NAD27_UTM_zone_16N",
        26717=>"PCS_NAD27_UTM_zone_17N",
        26718=>"PCS_NAD27_UTM_zone_18N",
        26719=>"PCS_NAD27_UTM_zone_19N",
        26720=>"PCS_NAD27_UTM_zone_20N",
        26721=>"PCS_NAD27_UTM_zone_21N",
        26722=>"PCS_NAD27_UTM_zone_22N",
        26729=>"PCS_NAD27_Alabama_East",
        26730=>"PCS_NAD27_Alabama_West",
        26731=>"PCS_NAD27_Alaska_zone_1",
        26732=>"PCS_NAD27_Alaska_zone_2",
        26733=>"PCS_NAD27_Alaska_zone_3",
        26734=>"PCS_NAD27_Alaska_zone_4",
        26735=>"PCS_NAD27_Alaska_zone_5",
        26736=>"PCS_NAD27_Alaska_zone_6",
        26737=>"PCS_NAD27_Alaska_zone_7",
        26738=>"PCS_NAD27_Alaska_zone_8",
        26739=>"PCS_NAD27_Alaska_zone_9",
        26740=>"PCS_NAD27_Alaska_zone_10",
        26741=>"PCS_NAD27_California_I",
        26742=>"PCS_NAD27_California_II",
        26743=>"PCS_NAD27_California_III",
        26744=>"PCS_NAD27_California_IV",
        26745=>"PCS_NAD27_California_V",
        26746=>"PCS_NAD27_California_VI",
        26747=>"PCS_NAD27_California_VII",
        26748=>"PCS_NAD27_Arizona_East",
        26749=>"PCS_NAD27_Arizona_Central",
        26750=>"PCS_NAD27_Arizona_West",
        26751=>"PCS_NAD27_Arkansas_North",
        26752=>"PCS_NAD27_Arkansas_South",
        26753=>"PCS_NAD27_Colorado_North",
        26754=>"PCS_NAD27_Colorado_Central",
        26755=>"PCS_NAD27_Colorado_South",
        26756=>"PCS_NAD27_Connecticut",
        26757=>"PCS_NAD27_Delaware",
        26758=>"PCS_NAD27_Florida_East",
        26759=>"PCS_NAD27_Florida_West",
        26760=>"PCS_NAD27_Florida_North",
        26761=>"PCS_NAD27_Hawaii_zone_1",
        26762=>"PCS_NAD27_Hawaii_zone_2",
        26763=>"PCS_NAD27_Hawaii_zone_3",
        26764=>"PCS_NAD27_Hawaii_zone_4",
        26765=>"PCS_NAD27_Hawaii_zone_5",
        26766=>"PCS_NAD27_Georgia_East",
        26767=>"PCS_NAD27_Georgia_West",
        26768=>"PCS_NAD27_Idaho_East",
        26769=>"PCS_NAD27_Idaho_Central",
        26770=>"PCS_NAD27_Idaho_West",
        26771=>"PCS_NAD27_Illinois_East",
        26772=>"PCS_NAD27_Illinois_West",
        26773=>"PCS_NAD27_Indiana_East",
        26774=>"PCS_NAD27_BLM_14N_feet",
        26775=>"PCS_NAD27_BLM_15N_feet",
        26776=>"PCS_NAD27_BLM_16N_feet",
        26777=>"PCS_NAD27_BLM_17N_feet",
        26778=>"PCS_NAD27_Kansas_South",
        26779=>"PCS_NAD27_Kentucky_North",
        26780=>"PCS_NAD27_Kentucky_South",
        26781=>"PCS_NAD27_Louisiana_North",
        26782=>"PCS_NAD27_Louisiana_South",
        26783=>"PCS_NAD27_Maine_East",
        26784=>"PCS_NAD27_Maine_West",
        26785=>"PCS_NAD27_Maryland",
        26786=>"PCS_NAD27_Massachusetts",
        26787=>"PCS_NAD27_Massachusetts_Is",
        26788=>"PCS_NAD27_Michigan_North",
        26789=>"PCS_NAD27_Michigan_Central",
        26790=>"PCS_NAD27_Michigan_South",
        26791=>"PCS_NAD27_Minnesota_North",
        26792=>"PCS_NAD27_Minnesota_Cent",
        26793=>"PCS_NAD27_Minnesota_South",
        26794=>"PCS_NAD27_Mississippi_East",
        26795=>"PCS_NAD27_Mississippi_West",
        26796=>"PCS_NAD27_Missouri_East",
        26797=>"PCS_NAD27_Missouri_Central",
        26798=>"PCS_NAD27_Missouri_West",
        26801=>"PCS_NAD_Michigan_Michigan_East",
        26802=>"PCS_NAD_Michigan_Michigan_Old_Central",
        26803=>"PCS_NAD_Michigan_Michigan_West",
        26903=>"PCS_NAD83_UTM_zone_3N",
        26904=>"PCS_NAD83_UTM_zone_4N",
        26905=>"PCS_NAD83_UTM_zone_5N",
        26906=>"PCS_NAD83_UTM_zone_6N",
        26907=>"PCS_NAD83_UTM_zone_7N",
        26908=>"PCS_NAD83_UTM_zone_8N",
        26909=>"PCS_NAD83_UTM_zone_9N",
        26910=>"PCS_NAD83_UTM_zone_10N",
        26911=>"PCS_NAD83_UTM_zone_11N",
        26912=>"PCS_NAD83_UTM_zone_12N",
        26913=>"PCS_NAD83_UTM_zone_13N",
        26914=>"PCS_NAD83_UTM_zone_14N",
        26915=>"PCS_NAD83_UTM_zone_15N",
        26916=>"PCS_NAD83_UTM_zone_16N",
        26917=>"PCS_NAD83_UTM_zone_17N",
        26918=>"PCS_NAD83_UTM_zone_18N",
        26919=>"PCS_NAD83_UTM_zone_19N",
        26920=>"PCS_NAD83_UTM_zone_20N",
        26921=>"PCS_NAD83_UTM_zone_21N",
        26922=>"PCS_NAD83_UTM_zone_22N",
        26923=>"PCS_NAD83_UTM_zone_23N",
        26929=>"PCS_NAD83_Alabama_East",
        26930=>"PCS_NAD83_Alabama_West",
        26931=>"PCS_NAD83_Alaska_zone_1",
        26932=>"PCS_NAD83_Alaska_zone_2",
        26933=>"PCS_NAD83_Alaska_zone_3",
        26934=>"PCS_NAD83_Alaska_zone_4",
        26935=>"PCS_NAD83_Alaska_zone_5",
        26936=>"PCS_NAD83_Alaska_zone_6",
        26937=>"PCS_NAD83_Alaska_zone_7",
        26938=>"PCS_NAD83_Alaska_zone_8",
        26939=>"PCS_NAD83_Alaska_zone_9",
        26940=>"PCS_NAD83_Alaska_zone_10",
        26941=>"PCS_NAD83_California_1",
        26942=>"PCS_NAD83_California_2",
        26943=>"PCS_NAD83_California_3",
        26944=>"PCS_NAD83_California_4",
        26945=>"PCS_NAD83_California_5",
        26946=>"PCS_NAD83_California_6",
        26948=>"PCS_NAD83_Arizona_East",
        26949=>"PCS_NAD83_Arizona_Central",
        26950=>"PCS_NAD83_Arizona_West",
        26951=>"PCS_NAD83_Arkansas_North",
        26952=>"PCS_NAD83_Arkansas_South",
        26953=>"PCS_NAD83_Colorado_North",
        26954=>"PCS_NAD83_Colorado_Central",
        26955=>"PCS_NAD83_Colorado_South",
        26956=>"PCS_NAD83_Connecticut",
        26957=>"PCS_NAD83_Delaware",
        26958=>"PCS_NAD83_Florida_East",
        26959=>"PCS_NAD83_Florida_West",
        26960=>"PCS_NAD83_Florida_North",
        26961=>"PCS_NAD83_Hawaii_zone_1",
        26962=>"PCS_NAD83_Hawaii_zone_2",
        26963=>"PCS_NAD83_Hawaii_zone_3",
        26964=>"PCS_NAD83_Hawaii_zone_4",
        26965=>"PCS_NAD83_Hawaii_zone_5",
        26966=>"PCS_NAD83_Georgia_East",
        26967=>"PCS_NAD83_Georgia_West",
        26968=>"PCS_NAD83_Idaho_East",
        26969=>"PCS_NAD83_Idaho_Central",
        26970=>"PCS_NAD83_Idaho_West",
        26971=>"PCS_NAD83_Illinois_East",
        26972=>"PCS_NAD83_Illinois_West",
        26973=>"PCS_NAD83_Indiana_East",
        26974=>"PCS_NAD83_Indiana_West",
        26975=>"PCS_NAD83_Iowa_North",
        26976=>"PCS_NAD83_Iowa_South",
        26977=>"PCS_NAD83_Kansas_North",
        26978=>"PCS_NAD83_Kansas_South",
        26979=>"PCS_NAD83_Kentucky_North",
        26980=>"PCS_NAD83_Kentucky_South",
        26981=>"PCS_NAD83_Louisiana_North",
        26982=>"PCS_NAD83_Louisiana_South",
        26983=>"PCS_NAD83_Maine_East",
        26984=>"PCS_NAD83_Maine_West",
        26985=>"PCS_NAD83_Maryland",
        26986=>"PCS_NAD83_Massachusetts",
        26987=>"PCS_NAD83_Massachusetts_Is",
        26988=>"PCS_NAD83_Michigan_North",
        26989=>"PCS_NAD83_Michigan_Central",
        26990=>"PCS_NAD83_Michigan_South",
        26991=>"PCS_NAD83_Minnesota_North",
        26992=>"PCS_NAD83_Minnesota_Cent",
        26993=>"PCS_NAD83_Minnesota_South",
        26994=>"PCS_NAD83_Mississippi_East",
        26995=>"PCS_NAD83_Mississippi_West",
        26996=>"PCS_NAD83_Missouri_East",
        26997=>"PCS_NAD83_Missouri_Central",
        26998=>"PCS_NAD83_Missouri_West",
        27038=>"PCS_Nahrwan_1967_UTM_38N",
        27039=>"PCS_Nahrwan_1967_UTM_39N",
        27040=>"PCS_Nahrwan_1967_UTM_40N",
        27120=>"PCS_Naparima_UTM_20N",
        27200=>"PCS_GD49_NZ_Map_Grid",
        27291=>"PCS_GD49_North_Island_Grid",
        27292=>"PCS_GD49_South_Island_Grid",
        27429=>"PCS_Datum_73_UTM_zone_29N",
        27500=>"PCS_ATF_Nord_de_Guerre",
        27581=>"PCS_NTF_France_I",
        27582=>"PCS_NTF_France_II",
        27583=>"PCS_NTF_France_III",
        27591=>"PCS_NTF_Nord_France",
        27592=>"PCS_NTF_Centre_France",
        27593=>"PCS_NTF_Sud_France",
        27700=>"PCS_British_National_Grid",
        28232=>"PCS_Point_Noire_UTM_32S",
        28348=>"PCS_GDA94_MGA_zone_48",
        28349=>"PCS_GDA94_MGA_zone_49",
        28350=>"PCS_GDA94_MGA_zone_50",
        28351=>"PCS_GDA94_MGA_zone_51",
        28352=>"PCS_GDA94_MGA_zone_52",
        28353=>"PCS_GDA94_MGA_zone_53",
        28354=>"PCS_GDA94_MGA_zone_54",
        28355=>"PCS_GDA94_MGA_zone_55",
        28356=>"PCS_GDA94_MGA_zone_56",
        28357=>"PCS_GDA94_MGA_zone_57",
        28358=>"PCS_GDA94_MGA_zone_58",
        28404=>"PCS_Pulkovo_Gauss_zone_4",
        28405=>"PCS_Pulkovo_Gauss_zone_5",
        28406=>"PCS_Pulkovo_Gauss_zone_6",
        28407=>"PCS_Pulkovo_Gauss_zone_7",
        28408=>"PCS_Pulkovo_Gauss_zone_8",
        28409=>"PCS_Pulkovo_Gauss_zone_9",
        28410=>"PCS_Pulkovo_Gauss_zone_10",
        28411=>"PCS_Pulkovo_Gauss_zone_11",
        28412=>"PCS_Pulkovo_Gauss_zone_12",
        28413=>"PCS_Pulkovo_Gauss_zone_13",
        28414=>"PCS_Pulkovo_Gauss_zone_14",
        28415=>"PCS_Pulkovo_Gauss_zone_15",
        28416=>"PCS_Pulkovo_Gauss_zone_16",
        28417=>"PCS_Pulkovo_Gauss_zone_17",
        28418=>"PCS_Pulkovo_Gauss_zone_18",
        28419=>"PCS_Pulkovo_Gauss_zone_19",
        28420=>"PCS_Pulkovo_Gauss_zone_20",
        28421=>"PCS_Pulkovo_Gauss_zone_21",
        28422=>"PCS_Pulkovo_Gauss_zone_22",
        28423=>"PCS_Pulkovo_Gauss_zone_23",
        28424=>"PCS_Pulkovo_Gauss_zone_24",
        28425=>"PCS_Pulkovo_Gauss_zone_25",
        28426=>"PCS_Pulkovo_Gauss_zone_26",
        28427=>"PCS_Pulkovo_Gauss_zone_27",
        28428=>"PCS_Pulkovo_Gauss_zone_28",
        28429=>"PCS_Pulkovo_Gauss_zone_29",
        28430=>"PCS_Pulkovo_Gauss_zone_30",
        28431=>"PCS_Pulkovo_Gauss_zone_31",
        28432=>"PCS_Pulkovo_Gauss_zone_32",
        28464=>"PCS_Pulkovo_Gauss_4N",
        28465=>"PCS_Pulkovo_Gauss_5N",
        28466=>"PCS_Pulkovo_Gauss_6N",
        28467=>"PCS_Pulkovo_Gauss_7N",
        28468=>"PCS_Pulkovo_Gauss_8N",
        28469=>"PCS_Pulkovo_Gauss_9N",
        28470=>"PCS_Pulkovo_Gauss_10N",
        28471=>"PCS_Pulkovo_Gauss_11N",
        28472=>"PCS_Pulkovo_Gauss_12N",
        28473=>"PCS_Pulkovo_Gauss_13N",
        28474=>"PCS_Pulkovo_Gauss_14N",
        28475=>"PCS_Pulkovo_Gauss_15N",
        28476=>"PCS_Pulkovo_Gauss_16N",
        28477=>"PCS_Pulkovo_Gauss_17N",
        28478=>"PCS_Pulkovo_Gauss_18N",
        28479=>"PCS_Pulkovo_Gauss_19N",
        28480=>"PCS_Pulkovo_Gauss_20N",
        28481=>"PCS_Pulkovo_Gauss_21N",
        28482=>"PCS_Pulkovo_Gauss_22N",
        28483=>"PCS_Pulkovo_Gauss_23N",
        28484=>"PCS_Pulkovo_Gauss_24N",
        28485=>"PCS_Pulkovo_Gauss_25N",
        28486=>"PCS_Pulkovo_Gauss_26N",
        28487=>"PCS_Pulkovo_Gauss_27N",
        28488=>"PCS_Pulkovo_Gauss_28N",
        28489=>"PCS_Pulkovo_Gauss_29N",
        28490=>"PCS_Pulkovo_Gauss_30N",
        28491=>"PCS_Pulkovo_Gauss_31N",
        28492=>"PCS_Pulkovo_Gauss_32N",
        28600=>"PCS_Qatar_National_Grid",
        28991=>"PCS_RD_Netherlands_Old",
        28992=>"PCS_RD_Netherlands_New",
        29118=>"PCS_SAD69_UTM_zone_18N",
        29119=>"PCS_SAD69_UTM_zone_19N",
        29120=>"PCS_SAD69_UTM_zone_20N",
        29121=>"PCS_SAD69_UTM_zone_21N",
        29122=>"PCS_SAD69_UTM_zone_22N",
        29177=>"PCS_SAD69_UTM_zone_17S",
        29178=>"PCS_SAD69_UTM_zone_18S",
        29179=>"PCS_SAD69_UTM_zone_19S",
        29180=>"PCS_SAD69_UTM_zone_20S",
        29181=>"PCS_SAD69_UTM_zone_21S",
        29182=>"PCS_SAD69_UTM_zone_22S",
        29183=>"PCS_SAD69_UTM_zone_23S",
        29184=>"PCS_SAD69_UTM_zone_24S",
        29185=>"PCS_SAD69_UTM_zone_25S",
        29220=>"PCS_Sapper_Hill_UTM_20S",
        29221=>"PCS_Sapper_Hill_UTM_21S",
        29333=>"PCS_Schwarzeck_UTM_33S",
        29635=>"PCS_Sudan_UTM_zone_35N",
        29636=>"PCS_Sudan_UTM_zone_36N",
        29700=>"PCS_Tananarive_Laborde",
        29738=>"PCS_Tananarive_UTM_38S",
        29739=>"PCS_Tananarive_UTM_39S",
        29800=>"PCS_Timbalai_1948_Borneo",
        29849=>"PCS_Timbalai_1948_UTM_49N",
        29850=>"PCS_Timbalai_1948_UTM_50N",
        29900=>"PCS_TM65_Irish_Nat_Grid",
        30200=>"PCS_Trinidad_1903_Trinidad",
        30339=>"PCS_TC_1948_UTM_zone_39N",
        30340=>"PCS_TC_1948_UTM_zone_40N",
        30491=>"PCS_Voirol_N_Algerie_ancien",
        30492=>"PCS_Voirol_S_Algerie_ancien",
        30591=>"PCS_Voirol_Unifie_N_Algerie",
        30592=>"PCS_Voirol_Unifie_S_Algerie",
        30600=>"PCS_Bern_1938_Swiss_New",
        30729=>"PCS_Nord_Sahara_UTM_29N",
        30730=>"PCS_Nord_Sahara_UTM_30N",
        30731=>"PCS_Nord_Sahara_UTM_31N",
        30732=>"PCS_Nord_Sahara_UTM_32N",
        31028=>"PCS_Yoff_UTM_zone_28N",
        31121=>"PCS_Zanderij_UTM_zone_21N",
        31291=>"PCS_MGI_Austria_West",
        31292=>"PCS_MGI_Austria_Central",
        31293=>"PCS_MGI_Austria_East",
        31300=>"PCS_Belge_Lambert_72",
        31491=>"PCS_DHDN_Germany_zone_1",
        31492=>"PCS_DHDN_Germany_zone_2",
        31493=>"PCS_DHDN_Germany_zone_3",
        31494=>"PCS_DHDN_Germany_zone_4",
        31495=>"PCS_DHDN_Germany_zone_5",
        32001=>"PCS_NAD27_Montana_North",
        32002=>"PCS_NAD27_Montana_Central",
        32003=>"PCS_NAD27_Montana_South",
        32005=>"PCS_NAD27_Nebraska_North",
        32006=>"PCS_NAD27_Nebraska_South",
        32007=>"PCS_NAD27_Nevada_East",
        32008=>"PCS_NAD27_Nevada_Central",
        32009=>"PCS_NAD27_Nevada_West",
        32010=>"PCS_NAD27_New_Hampshire",
        32011=>"PCS_NAD27_New_Jersey",
        32012=>"PCS_NAD27_New_Mexico_East",
        32013=>"PCS_NAD27_New_Mexico_Cent",
        32014=>"PCS_NAD27_New_Mexico_West",
        32015=>"PCS_NAD27_New_York_East",
        32016=>"PCS_NAD27_New_York_Central",
        32017=>"PCS_NAD27_New_York_West",
        32018=>"PCS_NAD27_New_York_Long_Is",
        32019=>"PCS_NAD27_North_Carolina",
        32020=>"PCS_NAD27_North_Dakota_N",
        32021=>"PCS_NAD27_North_Dakota_S",
        32022=>"PCS_NAD27_Ohio_North",
        32023=>"PCS_NAD27_Ohio_South",
        32024=>"PCS_NAD27_Oklahoma_North",
        32025=>"PCS_NAD27_Oklahoma_South",
        32026=>"PCS_NAD27_Oregon_North",
        32027=>"PCS_NAD27_Oregon_South",
        32028=>"PCS_NAD27_Pennsylvania_N",
        32029=>"PCS_NAD27_Pennsylvania_S",
        32030=>"PCS_NAD27_Rhode_Island",
        32031=>"PCS_NAD27_South_Carolina_N",
        32033=>"PCS_NAD27_South_Carolina_S",
        32034=>"PCS_NAD27_South_Dakota_N",
        32035=>"PCS_NAD27_South_Dakota_S",
        32036=>"PCS_NAD27_Tennessee",
        32037=>"PCS_NAD27_Texas_North",
        32038=>"PCS_NAD27_Texas_North_Cen",
        32039=>"PCS_NAD27_Texas_Central",
        32040=>"PCS_NAD27_Texas_South_Cen",
        32041=>"PCS_NAD27_Texas_South",
        32042=>"PCS_NAD27_Utah_North",
        32043=>"PCS_NAD27_Utah_Central",
        32044=>"PCS_NAD27_Utah_South",
        32045=>"PCS_NAD27_Vermont",
        32046=>"PCS_NAD27_Virginia_North",
        32047=>"PCS_NAD27_Virginia_South",
        32048=>"PCS_NAD27_Washington_North",
        32049=>"PCS_NAD27_Washington_South",
        32050=>"PCS_NAD27_West_Virginia_N",
        32051=>"PCS_NAD27_West_Virginia_S",
        32052=>"PCS_NAD27_Wisconsin_North",
        32053=>"PCS_NAD27_Wisconsin_Cen",
        32054=>"PCS_NAD27_Wisconsin_South",
        32055=>"PCS_NAD27_Wyoming_East",
        32056=>"PCS_NAD27_Wyoming_E_Cen",
        32057=>"PCS_NAD27_Wyoming_W_Cen",
        32058=>"PCS_NAD27_Wyoming_West",
        32059=>"PCS_NAD27_Puerto_Rico",
        32060=>"PCS_NAD27_St_Croix",
        32100=>"PCS_NAD83_Montana",
        32104=>"PCS_NAD83_Nebraska",
        32107=>"PCS_NAD83_Nevada_East",
        32108=>"PCS_NAD83_Nevada_Central",
        32109=>"PCS_NAD83_Nevada_West",
        32110=>"PCS_NAD83_New_Hampshire",
        32111=>"PCS_NAD83_New_Jersey",
        32112=>"PCS_NAD83_New_Mexico_East",
        32113=>"PCS_NAD83_New_Mexico_Cent",
        32114=>"PCS_NAD83_New_Mexico_West",
        32115=>"PCS_NAD83_New_York_East",
        32116=>"PCS_NAD83_New_York_Central",
        32117=>"PCS_NAD83_New_York_West",
        32118=>"PCS_NAD83_New_York_Long_Is",
        32119=>"PCS_NAD83_North_Carolina",
        32120=>"PCS_NAD83_North_Dakota_N",
        32121=>"PCS_NAD83_North_Dakota_S",
        32122=>"PCS_NAD83_Ohio_North",
        32123=>"PCS_NAD83_Ohio_South",
        32124=>"PCS_NAD83_Oklahoma_North",
        32125=>"PCS_NAD83_Oklahoma_South",
        32126=>"PCS_NAD83_Oregon_North",
        32127=>"PCS_NAD83_Oregon_South",
        32128=>"PCS_NAD83_Pennsylvania_N",
        32129=>"PCS_NAD83_Pennsylvania_S",
        32130=>"PCS_NAD83_Rhode_Island",
        32133=>"PCS_NAD83_South_Carolina",
        32134=>"PCS_NAD83_South_Dakota_N",
        32135=>"PCS_NAD83_South_Dakota_S",
        32136=>"PCS_NAD83_Tennessee",
        32137=>"PCS_NAD83_Texas_North",
        32138=>"PCS_NAD83_Texas_North_Cen",
        32139=>"PCS_NAD83_Texas_Central",
        32140=>"PCS_NAD83_Texas_South_Cen",
        32141=>"PCS_NAD83_Texas_South",
        32142=>"PCS_NAD83_Utah_North",
        32143=>"PCS_NAD83_Utah_Central",
        32144=>"PCS_NAD83_Utah_South",
        32145=>"PCS_NAD83_Vermont",
        32146=>"PCS_NAD83_Virginia_North",
        32147=>"PCS_NAD83_Virginia_South",
        32148=>"PCS_NAD83_Washington_North",
        32149=>"PCS_NAD83_Washington_South",
        32150=>"PCS_NAD83_West_Virginia_N",
        32151=>"PCS_NAD83_West_Virginia_S",
        32152=>"PCS_NAD83_Wisconsin_North",
        32153=>"PCS_NAD83_Wisconsin_Cen",
        32154=>"PCS_NAD83_Wisconsin_South",
        32155=>"PCS_NAD83_Wyoming_East",
        32156=>"PCS_NAD83_Wyoming_E_Cen",
        32157=>"PCS_NAD83_Wyoming_W_Cen",
        32158=>"PCS_NAD83_Wyoming_West",
        32161=>"PCS_NAD83_Puerto_Rico_Virgin_Is",
        32201=>"PCS_WGS72_UTM_zone_1N",
        32202=>"PCS_WGS72_UTM_zone_2N",
        32203=>"PCS_WGS72_UTM_zone_3N",
        32204=>"PCS_WGS72_UTM_zone_4N",
        32205=>"PCS_WGS72_UTM_zone_5N",
        32206=>"PCS_WGS72_UTM_zone_6N",
        32207=>"PCS_WGS72_UTM_zone_7N",
        32208=>"PCS_WGS72_UTM_zone_8N",
        32209=>"PCS_WGS72_UTM_zone_9N",
        32210=>"PCS_WGS72_UTM_zone_10N",
        32211=>"PCS_WGS72_UTM_zone_11N",
        32212=>"PCS_WGS72_UTM_zone_12N",
        32213=>"PCS_WGS72_UTM_zone_13N",
        32214=>"PCS_WGS72_UTM_zone_14N",
        32215=>"PCS_WGS72_UTM_zone_15N",
        32216=>"PCS_WGS72_UTM_zone_16N",
        32217=>"PCS_WGS72_UTM_zone_17N",
        32218=>"PCS_WGS72_UTM_zone_18N",
        32219=>"PCS_WGS72_UTM_zone_19N",
        32220=>"PCS_WGS72_UTM_zone_20N",
        32221=>"PCS_WGS72_UTM_zone_21N",
        32222=>"PCS_WGS72_UTM_zone_22N",
        32223=>"PCS_WGS72_UTM_zone_23N",
        32224=>"PCS_WGS72_UTM_zone_24N",
        32225=>"PCS_WGS72_UTM_zone_25N",
        32226=>"PCS_WGS72_UTM_zone_26N",
        32227=>"PCS_WGS72_UTM_zone_27N",
        32228=>"PCS_WGS72_UTM_zone_28N",
        32229=>"PCS_WGS72_UTM_zone_29N",
        32230=>"PCS_WGS72_UTM_zone_30N",
        32231=>"PCS_WGS72_UTM_zone_31N",
        32232=>"PCS_WGS72_UTM_zone_32N",
        32233=>"PCS_WGS72_UTM_zone_33N",
        32234=>"PCS_WGS72_UTM_zone_34N",
        32235=>"PCS_WGS72_UTM_zone_35N",
        32236=>"PCS_WGS72_UTM_zone_36N",
        32237=>"PCS_WGS72_UTM_zone_37N",
        32238=>"PCS_WGS72_UTM_zone_38N",
        32239=>"PCS_WGS72_UTM_zone_39N",
        32240=>"PCS_WGS72_UTM_zone_40N",
        32241=>"PCS_WGS72_UTM_zone_41N",
        32242=>"PCS_WGS72_UTM_zone_42N",
        32243=>"PCS_WGS72_UTM_zone_43N",
        32244=>"PCS_WGS72_UTM_zone_44N",
        32245=>"PCS_WGS72_UTM_zone_45N",
        32246=>"PCS_WGS72_UTM_zone_46N",
        32247=>"PCS_WGS72_UTM_zone_47N",
        32248=>"PCS_WGS72_UTM_zone_48N",
        32249=>"PCS_WGS72_UTM_zone_49N",
        32250=>"PCS_WGS72_UTM_zone_50N",
        32251=>"PCS_WGS72_UTM_zone_51N",
        32252=>"PCS_WGS72_UTM_zone_52N",
        32253=>"PCS_WGS72_UTM_zone_53N",
        32254=>"PCS_WGS72_UTM_zone_54N",
        32255=>"PCS_WGS72_UTM_zone_55N",
        32256=>"PCS_WGS72_UTM_zone_56N",
        32257=>"PCS_WGS72_UTM_zone_57N",
        32258=>"PCS_WGS72_UTM_zone_58N",
        32259=>"PCS_WGS72_UTM_zone_59N",
        32260=>"PCS_WGS72_UTM_zone_60N",
        32301=>"PCS_WGS72_UTM_zone_1S",
        32302=>"PCS_WGS72_UTM_zone_2S",
        32303=>"PCS_WGS72_UTM_zone_3S",
        32304=>"PCS_WGS72_UTM_zone_4S",
        32305=>"PCS_WGS72_UTM_zone_5S",
        32306=>"PCS_WGS72_UTM_zone_6S",
        32307=>"PCS_WGS72_UTM_zone_7S",
        32308=>"PCS_WGS72_UTM_zone_8S",
        32309=>"PCS_WGS72_UTM_zone_9S",
        32310=>"PCS_WGS72_UTM_zone_10S",
        32311=>"PCS_WGS72_UTM_zone_11S",
        32312=>"PCS_WGS72_UTM_zone_12S",
        32313=>"PCS_WGS72_UTM_zone_13S",
        32314=>"PCS_WGS72_UTM_zone_14S",
        32315=>"PCS_WGS72_UTM_zone_15S",
        32316=>"PCS_WGS72_UTM_zone_16S",
        32317=>"PCS_WGS72_UTM_zone_17S",
        32318=>"PCS_WGS72_UTM_zone_18S",
        32319=>"PCS_WGS72_UTM_zone_19S",
        32320=>"PCS_WGS72_UTM_zone_20S",
        32321=>"PCS_WGS72_UTM_zone_21S",
        32322=>"PCS_WGS72_UTM_zone_22S",
        32323=>"PCS_WGS72_UTM_zone_23S",
        32324=>"PCS_WGS72_UTM_zone_24S",
        32325=>"PCS_WGS72_UTM_zone_25S",
        32326=>"PCS_WGS72_UTM_zone_26S",
        32327=>"PCS_WGS72_UTM_zone_27S",
        32328=>"PCS_WGS72_UTM_zone_28S",
        32329=>"PCS_WGS72_UTM_zone_29S",
        32330=>"PCS_WGS72_UTM_zone_30S",
        32331=>"PCS_WGS72_UTM_zone_31S",
        32332=>"PCS_WGS72_UTM_zone_32S",
        32333=>"PCS_WGS72_UTM_zone_33S",
        32334=>"PCS_WGS72_UTM_zone_34S",
        32335=>"PCS_WGS72_UTM_zone_35S",
        32336=>"PCS_WGS72_UTM_zone_36S",
        32337=>"PCS_WGS72_UTM_zone_37S",
        32338=>"PCS_WGS72_UTM_zone_38S",
        32339=>"PCS_WGS72_UTM_zone_39S",
        32340=>"PCS_WGS72_UTM_zone_40S",
        32341=>"PCS_WGS72_UTM_zone_41S",
        32342=>"PCS_WGS72_UTM_zone_42S",
        32343=>"PCS_WGS72_UTM_zone_43S",
        32344=>"PCS_WGS72_UTM_zone_44S",
        32345=>"PCS_WGS72_UTM_zone_45S",
        32346=>"PCS_WGS72_UTM_zone_46S",
        32347=>"PCS_WGS72_UTM_zone_47S",
        32348=>"PCS_WGS72_UTM_zone_48S",
        32349=>"PCS_WGS72_UTM_zone_49S",
        32350=>"PCS_WGS72_UTM_zone_50S",
        32351=>"PCS_WGS72_UTM_zone_51S",
        32352=>"PCS_WGS72_UTM_zone_52S",
        32353=>"PCS_WGS72_UTM_zone_53S",
        32354=>"PCS_WGS72_UTM_zone_54S",
        32355=>"PCS_WGS72_UTM_zone_55S",
        32356=>"PCS_WGS72_UTM_zone_56S",
        32357=>"PCS_WGS72_UTM_zone_57S",
        32358=>"PCS_WGS72_UTM_zone_58S",
        32359=>"PCS_WGS72_UTM_zone_59S",
        32360=>"PCS_WGS72_UTM_zone_60S",
        32401=>"PCS_WGS72BE_UTM_zone_1N",
        32402=>"PCS_WGS72BE_UTM_zone_2N",
        32403=>"PCS_WGS72BE_UTM_zone_3N",
        32404=>"PCS_WGS72BE_UTM_zone_4N",
        32405=>"PCS_WGS72BE_UTM_zone_5N",
        32406=>"PCS_WGS72BE_UTM_zone_6N",
        32407=>"PCS_WGS72BE_UTM_zone_7N",
        32408=>"PCS_WGS72BE_UTM_zone_8N",
        32409=>"PCS_WGS72BE_UTM_zone_9N",
        32410=>"PCS_WGS72BE_UTM_zone_10N",
        32411=>"PCS_WGS72BE_UTM_zone_11N",
        32412=>"PCS_WGS72BE_UTM_zone_12N",
        32413=>"PCS_WGS72BE_UTM_zone_13N",
        32414=>"PCS_WGS72BE_UTM_zone_14N",
        32415=>"PCS_WGS72BE_UTM_zone_15N",
        32416=>"PCS_WGS72BE_UTM_zone_16N",
        32417=>"PCS_WGS72BE_UTM_zone_17N",
        32418=>"PCS_WGS72BE_UTM_zone_18N",
        32419=>"PCS_WGS72BE_UTM_zone_19N",
        32420=>"PCS_WGS72BE_UTM_zone_20N",
        32421=>"PCS_WGS72BE_UTM_zone_21N",
        32422=>"PCS_WGS72BE_UTM_zone_22N",
        32423=>"PCS_WGS72BE_UTM_zone_23N",
        32424=>"PCS_WGS72BE_UTM_zone_24N",
        32425=>"PCS_WGS72BE_UTM_zone_25N",
        32426=>"PCS_WGS72BE_UTM_zone_26N",
        32427=>"PCS_WGS72BE_UTM_zone_27N",
        32428=>"PCS_WGS72BE_UTM_zone_28N",
        32429=>"PCS_WGS72BE_UTM_zone_29N",
        32430=>"PCS_WGS72BE_UTM_zone_30N",
        32431=>"PCS_WGS72BE_UTM_zone_31N",
        32432=>"PCS_WGS72BE_UTM_zone_32N",
        32433=>"PCS_WGS72BE_UTM_zone_33N",
        32434=>"PCS_WGS72BE_UTM_zone_34N",
        32435=>"PCS_WGS72BE_UTM_zone_35N",
        32436=>"PCS_WGS72BE_UTM_zone_36N",
        32437=>"PCS_WGS72BE_UTM_zone_37N",
        32438=>"PCS_WGS72BE_UTM_zone_38N",
        32439=>"PCS_WGS72BE_UTM_zone_39N",
        32440=>"PCS_WGS72BE_UTM_zone_40N",
        32441=>"PCS_WGS72BE_UTM_zone_41N",
        32442=>"PCS_WGS72BE_UTM_zone_42N",
        32443=>"PCS_WGS72BE_UTM_zone_43N",
        32444=>"PCS_WGS72BE_UTM_zone_44N",
        32445=>"PCS_WGS72BE_UTM_zone_45N",
        32446=>"PCS_WGS72BE_UTM_zone_46N",
        32447=>"PCS_WGS72BE_UTM_zone_47N",
        32448=>"PCS_WGS72BE_UTM_zone_48N",
        32449=>"PCS_WGS72BE_UTM_zone_49N",
        32450=>"PCS_WGS72BE_UTM_zone_50N",
        32451=>"PCS_WGS72BE_UTM_zone_51N",
        32452=>"PCS_WGS72BE_UTM_zone_52N",
        32453=>"PCS_WGS72BE_UTM_zone_53N",
        32454=>"PCS_WGS72BE_UTM_zone_54N",
        32455=>"PCS_WGS72BE_UTM_zone_55N",
        32456=>"PCS_WGS72BE_UTM_zone_56N",
        32457=>"PCS_WGS72BE_UTM_zone_57N",
        32458=>"PCS_WGS72BE_UTM_zone_58N",
        32459=>"PCS_WGS72BE_UTM_zone_59N",
        32460=>"PCS_WGS72BE_UTM_zone_60N",
        32501=>"PCS_WGS72BE_UTM_zone_1S",
        32502=>"PCS_WGS72BE_UTM_zone_2S",
        32503=>"PCS_WGS72BE_UTM_zone_3S",
        32504=>"PCS_WGS72BE_UTM_zone_4S",
        32505=>"PCS_WGS72BE_UTM_zone_5S",
        32506=>"PCS_WGS72BE_UTM_zone_6S",
        32507=>"PCS_WGS72BE_UTM_zone_7S",
        32508=>"PCS_WGS72BE_UTM_zone_8S",
        32509=>"PCS_WGS72BE_UTM_zone_9S",
        32510=>"PCS_WGS72BE_UTM_zone_10S",
        32511=>"PCS_WGS72BE_UTM_zone_11S",
        32512=>"PCS_WGS72BE_UTM_zone_12S",
        32513=>"PCS_WGS72BE_UTM_zone_13S",
        32514=>"PCS_WGS72BE_UTM_zone_14S",
        32515=>"PCS_WGS72BE_UTM_zone_15S",
        32516=>"PCS_WGS72BE_UTM_zone_16S",
        32517=>"PCS_WGS72BE_UTM_zone_17S",
        32518=>"PCS_WGS72BE_UTM_zone_18S",
        32519=>"PCS_WGS72BE_UTM_zone_19S",
        32520=>"PCS_WGS72BE_UTM_zone_20S",
        32521=>"PCS_WGS72BE_UTM_zone_21S",
        32522=>"PCS_WGS72BE_UTM_zone_22S",
        32523=>"PCS_WGS72BE_UTM_zone_23S",
        32524=>"PCS_WGS72BE_UTM_zone_24S",
        32525=>"PCS_WGS72BE_UTM_zone_25S",
        32526=>"PCS_WGS72BE_UTM_zone_26S",
        32527=>"PCS_WGS72BE_UTM_zone_27S",
        32528=>"PCS_WGS72BE_UTM_zone_28S",
        32529=>"PCS_WGS72BE_UTM_zone_29S",
        32530=>"PCS_WGS72BE_UTM_zone_30S",
        32531=>"PCS_WGS72BE_UTM_zone_31S",
        32532=>"PCS_WGS72BE_UTM_zone_32S",
        32533=>"PCS_WGS72BE_UTM_zone_33S",
        32534=>"PCS_WGS72BE_UTM_zone_34S",
        32535=>"PCS_WGS72BE_UTM_zone_35S",
        32536=>"PCS_WGS72BE_UTM_zone_36S",
        32537=>"PCS_WGS72BE_UTM_zone_37S",
        32538=>"PCS_WGS72BE_UTM_zone_38S",
        32539=>"PCS_WGS72BE_UTM_zone_39S",
        32540=>"PCS_WGS72BE_UTM_zone_40S",
        32541=>"PCS_WGS72BE_UTM_zone_41S",
        32542=>"PCS_WGS72BE_UTM_zone_42S",
        32543=>"PCS_WGS72BE_UTM_zone_43S",
        32544=>"PCS_WGS72BE_UTM_zone_44S",
        32545=>"PCS_WGS72BE_UTM_zone_45S",
        32546=>"PCS_WGS72BE_UTM_zone_46S",
        32547=>"PCS_WGS72BE_UTM_zone_47S",
        32548=>"PCS_WGS72BE_UTM_zone_48S",
        32549=>"PCS_WGS72BE_UTM_zone_49S",
        32550=>"PCS_WGS72BE_UTM_zone_50S",
        32551=>"PCS_WGS72BE_UTM_zone_51S",
        32552=>"PCS_WGS72BE_UTM_zone_52S",
        32553=>"PCS_WGS72BE_UTM_zone_53S",
        32554=>"PCS_WGS72BE_UTM_zone_54S",
        32555=>"PCS_WGS72BE_UTM_zone_55S",
        32556=>"PCS_WGS72BE_UTM_zone_56S",
        32557=>"PCS_WGS72BE_UTM_zone_57S",
        32558=>"PCS_WGS72BE_UTM_zone_58S",
        32559=>"PCS_WGS72BE_UTM_zone_59S",
        32560=>"PCS_WGS72BE_UTM_zone_60S",
        32601=>"PCS_WGS84_UTM_zone_1N",
        32602=>"PCS_WGS84_UTM_zone_2N",
        32603=>"PCS_WGS84_UTM_zone_3N",
        32604=>"PCS_WGS84_UTM_zone_4N",
        32605=>"PCS_WGS84_UTM_zone_5N",
        32606=>"PCS_WGS84_UTM_zone_6N",
        32607=>"PCS_WGS84_UTM_zone_7N",
        32608=>"PCS_WGS84_UTM_zone_8N",
        32609=>"PCS_WGS84_UTM_zone_9N",
        32610=>"PCS_WGS84_UTM_zone_10N",
        32611=>"PCS_WGS84_UTM_zone_11N",
        32612=>"PCS_WGS84_UTM_zone_12N",
        32613=>"PCS_WGS84_UTM_zone_13N",
        32614=>"PCS_WGS84_UTM_zone_14N",
        32615=>"PCS_WGS84_UTM_zone_15N",
        32616=>"PCS_WGS84_UTM_zone_16N",
        32617=>"PCS_WGS84_UTM_zone_17N",
        32618=>"PCS_WGS84_UTM_zone_18N",
        32619=>"PCS_WGS84_UTM_zone_19N",
        32620=>"PCS_WGS84_UTM_zone_20N",
        32621=>"PCS_WGS84_UTM_zone_21N",
        32622=>"PCS_WGS84_UTM_zone_22N",
        32623=>"PCS_WGS84_UTM_zone_23N",
        32624=>"PCS_WGS84_UTM_zone_24N",
        32625=>"PCS_WGS84_UTM_zone_25N",
        32626=>"PCS_WGS84_UTM_zone_26N",
        32627=>"PCS_WGS84_UTM_zone_27N",
        32628=>"PCS_WGS84_UTM_zone_28N",
        32629=>"PCS_WGS84_UTM_zone_29N",
        32630=>"PCS_WGS84_UTM_zone_30N",
        32631=>"PCS_WGS84_UTM_zone_31N",
        32632=>"PCS_WGS84_UTM_zone_32N",
        32633=>"PCS_WGS84_UTM_zone_33N",
        32634=>"PCS_WGS84_UTM_zone_34N",
        32635=>"PCS_WGS84_UTM_zone_35N",
        32636=>"PCS_WGS84_UTM_zone_36N",
        32637=>"PCS_WGS84_UTM_zone_37N",
        32638=>"PCS_WGS84_UTM_zone_38N",
        32639=>"PCS_WGS84_UTM_zone_39N",
        32640=>"PCS_WGS84_UTM_zone_40N",
        32641=>"PCS_WGS84_UTM_zone_41N",
        32642=>"PCS_WGS84_UTM_zone_42N",
        32643=>"PCS_WGS84_UTM_zone_43N",
        32644=>"PCS_WGS84_UTM_zone_44N",
        32645=>"PCS_WGS84_UTM_zone_45N",
        32646=>"PCS_WGS84_UTM_zone_46N",
        32647=>"PCS_WGS84_UTM_zone_47N",
        32648=>"PCS_WGS84_UTM_zone_48N",
        32649=>"PCS_WGS84_UTM_zone_49N",
        32650=>"PCS_WGS84_UTM_zone_50N",
        32651=>"PCS_WGS84_UTM_zone_51N",
        32652=>"PCS_WGS84_UTM_zone_52N",
        32653=>"PCS_WGS84_UTM_zone_53N",
        32654=>"PCS_WGS84_UTM_zone_54N",
        32655=>"PCS_WGS84_UTM_zone_55N",
        32656=>"PCS_WGS84_UTM_zone_56N",
        32657=>"PCS_WGS84_UTM_zone_57N",
        32658=>"PCS_WGS84_UTM_zone_58N",
        32659=>"PCS_WGS84_UTM_zone_59N",
        32660=>"PCS_WGS84_UTM_zone_60N",
        32701=>"PCS_WGS84_UTM_zone_1S",
        32702=>"PCS_WGS84_UTM_zone_2S",
        32703=>"PCS_WGS84_UTM_zone_3S",
        32704=>"PCS_WGS84_UTM_zone_4S",
        32705=>"PCS_WGS84_UTM_zone_5S",
        32706=>"PCS_WGS84_UTM_zone_6S",
        32707=>"PCS_WGS84_UTM_zone_7S",
        32708=>"PCS_WGS84_UTM_zone_8S",
        32709=>"PCS_WGS84_UTM_zone_9S",
        32710=>"PCS_WGS84_UTM_zone_10S",
        32711=>"PCS_WGS84_UTM_zone_11S",
        32712=>"PCS_WGS84_UTM_zone_12S",
        32713=>"PCS_WGS84_UTM_zone_13S",
        32714=>"PCS_WGS84_UTM_zone_14S",
        32715=>"PCS_WGS84_UTM_zone_15S",
        32716=>"PCS_WGS84_UTM_zone_16S",
        32717=>"PCS_WGS84_UTM_zone_17S",
        32718=>"PCS_WGS84_UTM_zone_18S",
        32719=>"PCS_WGS84_UTM_zone_19S",
        32720=>"PCS_WGS84_UTM_zone_20S",
        32721=>"PCS_WGS84_UTM_zone_21S",
        32722=>"PCS_WGS84_UTM_zone_22S",
        32723=>"PCS_WGS84_UTM_zone_23S",
        32724=>"PCS_WGS84_UTM_zone_24S",
        32725=>"PCS_WGS84_UTM_zone_25S",
        32726=>"PCS_WGS84_UTM_zone_26S",
        32727=>"PCS_WGS84_UTM_zone_27S",
        32728=>"PCS_WGS84_UTM_zone_28S",
        32729=>"PCS_WGS84_UTM_zone_29S",
        32730=>"PCS_WGS84_UTM_zone_30S",
        32731=>"PCS_WGS84_UTM_zone_31S",
        32732=>"PCS_WGS84_UTM_zone_32S",
        32733=>"PCS_WGS84_UTM_zone_33S",
        32734=>"PCS_WGS84_UTM_zone_34S",
        32735=>"PCS_WGS84_UTM_zone_35S",
        32736=>"PCS_WGS84_UTM_zone_36S",
        32737=>"PCS_WGS84_UTM_zone_37S",
        32738=>"PCS_WGS84_UTM_zone_38S",
        32739=>"PCS_WGS84_UTM_zone_39S",
        32740=>"PCS_WGS84_UTM_zone_40S",
        32741=>"PCS_WGS84_UTM_zone_41S",
        32742=>"PCS_WGS84_UTM_zone_42S",
        32743=>"PCS_WGS84_UTM_zone_43S",
        32744=>"PCS_WGS84_UTM_zone_44S",
        32745=>"PCS_WGS84_UTM_zone_45S",
        32746=>"PCS_WGS84_UTM_zone_46S",
        32747=>"PCS_WGS84_UTM_zone_47S",
        32748=>"PCS_WGS84_UTM_zone_48S",
        32749=>"PCS_WGS84_UTM_zone_49S",
        32750=>"PCS_WGS84_UTM_zone_50S",
        32751=>"PCS_WGS84_UTM_zone_51S",
        32752=>"PCS_WGS84_UTM_zone_52S",
        32753=>"PCS_WGS84_UTM_zone_53S",
        32754=>"PCS_WGS84_UTM_zone_54S",
        32755=>"PCS_WGS84_UTM_zone_55S",
        32756=>"PCS_WGS84_UTM_zone_56S",
        32757=>"PCS_WGS84_UTM_zone_57S",
        32758=>"PCS_WGS84_UTM_zone_58S",
        32759=>"PCS_WGS84_UTM_zone_59S",
        32760=>"PCS_WGS84_UTM_zone_60S"
    ];
    kw.insert(3072u16, projected_cs_type_map);

    let proj_map = hashmap![
        10101=>"Proj_Alabama_CS27_East",
        10102=>"Proj_Alabama_CS27_West",
        10131=>"Proj_Alabama_CS83_East",
        10132=>"Proj_Alabama_CS83_West",
        10201=>"Proj_Arizona_Coordinate_System_east",
        10202=>"Proj_Arizona_Coordinate_System_Central",
        10203=>"Proj_Arizona_Coordinate_System_west",
        10231=>"Proj_Arizona_CS83_east",
        10232=>"Proj_Arizona_CS83_Central",
        10233=>"Proj_Arizona_CS83_west",
        10301=>"Proj_Arkansas_CS27_North",
        10302=>"Proj_Arkansas_CS27_South",
        10331=>"Proj_Arkansas_CS83_North",
        10332=>"Proj_Arkansas_CS83_South",
        10401=>"Proj_California_CS27_I",
        10402=>"Proj_California_CS27_II",
        10403=>"Proj_California_CS27_III",
        10404=>"Proj_California_CS27_IV",
        10405=>"Proj_California_CS27_V",
        10406=>"Proj_California_CS27_VI",
        10407=>"Proj_California_CS27_VII",
        10431=>"Proj_California_CS83_1",
        10432=>"Proj_California_CS83_2",
        10433=>"Proj_California_CS83_3",
        10434=>"Proj_California_CS83_4",
        10435=>"Proj_California_CS83_5",
        10436=>"Proj_California_CS83_6",
        10501=>"Proj_Colorado_CS27_North",
        10502=>"Proj_Colorado_CS27_Central",
        10503=>"Proj_Colorado_CS27_South",
        10531=>"Proj_Colorado_CS83_North",
        10532=>"Proj_Colorado_CS83_Central",
        10533=>"Proj_Colorado_CS83_South",
        10600=>"Proj_Connecticut_CS27",
        10630=>"Proj_Connecticut_CS83",
        10700=>"Proj_Delaware_CS27",
        10730=>"Proj_Delaware_CS83",
        10901=>"Proj_Florida_CS27_East",
        10902=>"Proj_Florida_CS27_West",
        10903=>"Proj_Florida_CS27_North",
        10931=>"Proj_Florida_CS83_East",
        10932=>"Proj_Florida_CS83_West",
        10933=>"Proj_Florida_CS83_North",
        11001=>"Proj_Georgia_CS27_East",
        11002=>"Proj_Georgia_CS27_West",
        11031=>"Proj_Georgia_CS83_East",
        11032=>"Proj_Georgia_CS83_West",
        11101=>"Proj_Idaho_CS27_East",
        11102=>"Proj_Idaho_CS27_Central",
        11103=>"Proj_Idaho_CS27_West",
        11131=>"Proj_Idaho_CS83_East",
        11132=>"Proj_Idaho_CS83_Central",
        11133=>"Proj_Idaho_CS83_West",
        11201=>"Proj_Illinois_CS27_East",
        11202=>"Proj_Illinois_CS27_West",
        11231=>"Proj_Illinois_CS83_East",
        11232=>"Proj_Illinois_CS83_West",
        11301=>"Proj_Indiana_CS27_East",
        11302=>"Proj_Indiana_CS27_West",
        11331=>"Proj_Indiana_CS83_East",
        11332=>"Proj_Indiana_CS83_West",
        11401=>"Proj_Iowa_CS27_North",
        11402=>"Proj_Iowa_CS27_South",
        11431=>"Proj_Iowa_CS83_North",
        11432=>"Proj_Iowa_CS83_South",
        11501=>"Proj_Kansas_CS27_North",
        11502=>"Proj_Kansas_CS27_South",
        11531=>"Proj_Kansas_CS83_North",
        11532=>"Proj_Kansas_CS83_South",
        11601=>"Proj_Kentucky_CS27_North",
        11602=>"Proj_Kentucky_CS27_South",
        11631=>"Proj_Kentucky_CS83_North",
        11632=>"Proj_Kentucky_CS83_South",
        11701=>"Proj_Louisiana_CS27_North",
        11702=>"Proj_Louisiana_CS27_South",
        11731=>"Proj_Louisiana_CS83_North",
        11732=>"Proj_Louisiana_CS83_South",
        11801=>"Proj_Maine_CS27_East",
        11802=>"Proj_Maine_CS27_West",
        11831=>"Proj_Maine_CS83_East",
        11832=>"Proj_Maine_CS83_West",
        11900=>"Proj_Maryland_CS27",
        11930=>"Proj_Maryland_CS83",
        12001=>"Proj_Massachusetts_CS27_Mainland",
        12002=>"Proj_Massachusetts_CS27_Island",
        12031=>"Proj_Massachusetts_CS83_Mainland",
        12032=>"Proj_Massachusetts_CS83_Island",
        12101=>"Proj_Michigan_State_Plane_East",
        12102=>"Proj_Michigan_State_Plane_Old_Central",
        12103=>"Proj_Michigan_State_Plane_West",
        12111=>"Proj_Michigan_CS27_North",
        12112=>"Proj_Michigan_CS27_Central",
        12113=>"Proj_Michigan_CS27_South",
        12141=>"Proj_Michigan_CS83_North",
        12142=>"Proj_Michigan_CS83_Central",
        12143=>"Proj_Michigan_CS83_South",
        12201=>"Proj_Minnesota_CS27_North",
        12202=>"Proj_Minnesota_CS27_Central",
        12203=>"Proj_Minnesota_CS27_South",
        12231=>"Proj_Minnesota_CS83_North",
        12232=>"Proj_Minnesota_CS83_Central",
        12233=>"Proj_Minnesota_CS83_South",
        12301=>"Proj_Mississippi_CS27_East",
        12302=>"Proj_Mississippi_CS27_West",
        12331=>"Proj_Mississippi_CS83_East",
        12332=>"Proj_Mississippi_CS83_West",
        12401=>"Proj_Missouri_CS27_East",
        12402=>"Proj_Missouri_CS27_Central",
        12403=>"Proj_Missouri_CS27_West",
        12431=>"Proj_Missouri_CS83_East",
        12432=>"Proj_Missouri_CS83_Central",
        12433=>"Proj_Missouri_CS83_West",
        12501=>"Proj_Montana_CS27_North",
        12502=>"Proj_Montana_CS27_Central",
        12503=>"Proj_Montana_CS27_South",
        12530=>"Proj_Montana_CS83",
        12601=>"Proj_Nebraska_CS27_North",
        12602=>"Proj_Nebraska_CS27_South",
        12630=>"Proj_Nebraska_CS83",
        12701=>"Proj_Nevada_CS27_East",
        12702=>"Proj_Nevada_CS27_Central",
        12703=>"Proj_Nevada_CS27_West",
        12731=>"Proj_Nevada_CS83_East",
        12732=>"Proj_Nevada_CS83_Central",
        12733=>"Proj_Nevada_CS83_West",
        12800=>"Proj_New_Hampshire_CS27",
        12830=>"Proj_New_Hampshire_CS83",
        12900=>"Proj_New_Jersey_CS27",
        12930=>"Proj_New_Jersey_CS83",
        13001=>"Proj_New_Mexico_CS27_East",
        13002=>"Proj_New_Mexico_CS27_Central",
        13003=>"Proj_New_Mexico_CS27_West",
        13031=>"Proj_New_Mexico_CS83_East",
        13032=>"Proj_New_Mexico_CS83_Central",
        13033=>"Proj_New_Mexico_CS83_West",
        13101=>"Proj_New_York_CS27_East",
        13102=>"Proj_New_York_CS27_Central",
        13103=>"Proj_New_York_CS27_West",
        13104=>"Proj_New_York_CS27_Long_Island",
        13131=>"Proj_New_York_CS83_East",
        13132=>"Proj_New_York_CS83_Central",
        13133=>"Proj_New_York_CS83_West",
        13134=>"Proj_New_York_CS83_Long_Island",
        13200=>"Proj_North_Carolina_CS27",
        13230=>"Proj_North_Carolina_CS83",
        13301=>"Proj_North_Dakota_CS27_North",
        13302=>"Proj_North_Dakota_CS27_South",
        13331=>"Proj_North_Dakota_CS83_North",
        13332=>"Proj_North_Dakota_CS83_South",
        13401=>"Proj_Ohio_CS27_North",
        13402=>"Proj_Ohio_CS27_South",
        13431=>"Proj_Ohio_CS83_North",
        13432=>"Proj_Ohio_CS83_South",
        13501=>"Proj_Oklahoma_CS27_North",
        13502=>"Proj_Oklahoma_CS27_South",
        13531=>"Proj_Oklahoma_CS83_North",
        13532=>"Proj_Oklahoma_CS83_South",
        13601=>"Proj_Oregon_CS27_North",
        13602=>"Proj_Oregon_CS27_South",
        13631=>"Proj_Oregon_CS83_North",
        13632=>"Proj_Oregon_CS83_South",
        13701=>"Proj_Pennsylvania_CS27_North",
        13702=>"Proj_Pennsylvania_CS27_South",
        13731=>"Proj_Pennsylvania_CS83_North",
        13732=>"Proj_Pennsylvania_CS83_South",
        13800=>"Proj_Rhode_Island_CS27",
        13830=>"Proj_Rhode_Island_CS83",
        13901=>"Proj_South_Carolina_CS27_North",
        13902=>"Proj_South_Carolina_CS27_South",
        13930=>"Proj_South_Carolina_CS83",
        14001=>"Proj_South_Dakota_CS27_North",
        14002=>"Proj_South_Dakota_CS27_South",
        14031=>"Proj_South_Dakota_CS83_North",
        14032=>"Proj_South_Dakota_CS83_South",
        14100=>"Proj_Tennessee_CS27",
        14130=>"Proj_Tennessee_CS83",
        14201=>"Proj_Texas_CS27_North",
        14202=>"Proj_Texas_CS27_North_Central",
        14203=>"Proj_Texas_CS27_Central",
        14204=>"Proj_Texas_CS27_South_Central",
        14205=>"Proj_Texas_CS27_South",
        14231=>"Proj_Texas_CS83_North",
        14232=>"Proj_Texas_CS83_North_Central",
        14233=>"Proj_Texas_CS83_Central",
        14234=>"Proj_Texas_CS83_South_Central",
        14235=>"Proj_Texas_CS83_South",
        14301=>"Proj_Utah_CS27_North",
        14302=>"Proj_Utah_CS27_Central",
        14303=>"Proj_Utah_CS27_South",
        14331=>"Proj_Utah_CS83_North",
        14332=>"Proj_Utah_CS83_Central",
        14333=>"Proj_Utah_CS83_South",
        14400=>"Proj_Vermont_CS27",
        14430=>"Proj_Vermont_CS83",
        14501=>"Proj_Virginia_CS27_North",
        14502=>"Proj_Virginia_CS27_South",
        14531=>"Proj_Virginia_CS83_North",
        14532=>"Proj_Virginia_CS83_South",
        14601=>"Proj_Washington_CS27_North",
        14602=>"Proj_Washington_CS27_South",
        14631=>"Proj_Washington_CS83_North",
        14632=>"Proj_Washington_CS83_South",
        14701=>"Proj_West_Virginia_CS27_North",
        14702=>"Proj_West_Virginia_CS27_South",
        14731=>"Proj_West_Virginia_CS83_North",
        14732=>"Proj_West_Virginia_CS83_South",
        14801=>"Proj_Wisconsin_CS27_North",
        14802=>"Proj_Wisconsin_CS27_Central",
        14803=>"Proj_Wisconsin_CS27_South",
        14831=>"Proj_Wisconsin_CS83_North",
        14832=>"Proj_Wisconsin_CS83_Central",
        14833=>"Proj_Wisconsin_CS83_South",
        14901=>"Proj_Wyoming_CS27_East",
        14902=>"Proj_Wyoming_CS27_East_Central",
        14903=>"Proj_Wyoming_CS27_West_Central",
        14904=>"Proj_Wyoming_CS27_West",
        14931=>"Proj_Wyoming_CS83_East",
        14932=>"Proj_Wyoming_CS83_East_Central",
        14933=>"Proj_Wyoming_CS83_West_Central",
        14934=>"Proj_Wyoming_CS83_West",
        15001=>"Proj_Alaska_CS27_1",
        15002=>"Proj_Alaska_CS27_2",
        15003=>"Proj_Alaska_CS27_3",
        15004=>"Proj_Alaska_CS27_4",
        15005=>"Proj_Alaska_CS27_5",
        15006=>"Proj_Alaska_CS27_6",
        15007=>"Proj_Alaska_CS27_7",
        15008=>"Proj_Alaska_CS27_8",
        15009=>"Proj_Alaska_CS27_9",
        15010=>"Proj_Alaska_CS27_10",
        15031=>"Proj_Alaska_CS83_1",
        15032=>"Proj_Alaska_CS83_2",
        15033=>"Proj_Alaska_CS83_3",
        15034=>"Proj_Alaska_CS83_4",
        15035=>"Proj_Alaska_CS83_5",
        15036=>"Proj_Alaska_CS83_6",
        15037=>"Proj_Alaska_CS83_7",
        15038=>"Proj_Alaska_CS83_8",
        15039=>"Proj_Alaska_CS83_9",
        15040=>"Proj_Alaska_CS83_10",
        15101=>"Proj_Hawaii_CS27_1",
        15102=>"Proj_Hawaii_CS27_2",
        15103=>"Proj_Hawaii_CS27_3",
        15104=>"Proj_Hawaii_CS27_4",
        15105=>"Proj_Hawaii_CS27_5",
        15131=>"Proj_Hawaii_CS83_1",
        15132=>"Proj_Hawaii_CS83_2",
        15133=>"Proj_Hawaii_CS83_3",
        15134=>"Proj_Hawaii_CS83_4",
        15135=>"Proj_Hawaii_CS83_5",
        15201=>"Proj_Puerto_Rico_CS27",
        15202=>"Proj_St_Croix",
        15230=>"Proj_Puerto_Rico_Virgin_Is",
        15914=>"Proj_BLM_14N_feet",
        15915=>"Proj_BLM_15N_feet",
        15916=>"Proj_BLM_16N_feet",
        15917=>"Proj_BLM_17N_feet",
        17348=>"Proj_Map_Grid_of_Australia_48",
        17349=>"Proj_Map_Grid_of_Australia_49",
        17350=>"Proj_Map_Grid_of_Australia_50",
        17351=>"Proj_Map_Grid_of_Australia_51",
        17352=>"Proj_Map_Grid_of_Australia_52",
        17353=>"Proj_Map_Grid_of_Australia_53",
        17354=>"Proj_Map_Grid_of_Australia_54",
        17355=>"Proj_Map_Grid_of_Australia_55",
        17356=>"Proj_Map_Grid_of_Australia_56",
        17357=>"Proj_Map_Grid_of_Australia_57",
        17358=>"Proj_Map_Grid_of_Australia_58",
        17448=>"Proj_Australian_Map_Grid_48",
        17449=>"Proj_Australian_Map_Grid_49",
        17450=>"Proj_Australian_Map_Grid_50",
        17451=>"Proj_Australian_Map_Grid_51",
        17452=>"Proj_Australian_Map_Grid_52",
        17453=>"Proj_Australian_Map_Grid_53",
        17454=>"Proj_Australian_Map_Grid_54",
        17455=>"Proj_Australian_Map_Grid_55",
        17456=>"Proj_Australian_Map_Grid_56",
        17457=>"Proj_Australian_Map_Grid_57",
        17458=>"Proj_Australian_Map_Grid_58",
        18031=>"Proj_Argentina_1",
        18032=>"Proj_Argentina_2",
        18033=>"Proj_Argentina_3",
        18034=>"Proj_Argentina_4",
        18035=>"Proj_Argentina_5",
        18036=>"Proj_Argentina_6",
        18037=>"Proj_Argentina_7",
        18051=>"Proj_Colombia_3W",
        18052=>"Proj_Colombia_Bogota",
        18053=>"Proj_Colombia_3E",
        18054=>"Proj_Colombia_6E",
        18072=>"Proj_Egypt_Red_Belt",
        18073=>"Proj_Egypt_Purple_Belt",
        18074=>"Proj_Extended_Purple_Belt",
        18141=>"Proj_New_Zealand_North_Island_Nat_Grid",
        18142=>"Proj_New_Zealand_South_Island_Nat_Grid",
        19900=>"Proj_Bahrain_Grid",
        19905=>"Proj_Netherlands_E_Indies_Equatorial",
        19912=>"Proj_RSO_Borneo"
    ];
    kw.insert(3074u16, proj_map);

    let proj_coord_trans_map = hashmap![
        1u16=>"CT_TransverseMercator",
        2=>"CT_TransvMercator_Modified_Alaska",
        3=>"CT_ObliqueMercator",
        4=>"CT_ObliqueMercator_Laborde",
        5=>"CT_ObliqueMercator_Rosenmund",
        6=>"CT_ObliqueMercator_Spherical",
        7=>"CT_Mercator",
        8=>"CT_LambertConfConic_2SP",
        9=>"CT_LambertConfConic_Helmert",
        10=>"CT_LambertAzimEqualArea",
        11=>"CT_AlbersEqualArea",
        12=>"CT_AzimuthalEquidistant",
        13=>"CT_EquidistantConic",
        14=>"CT_Stereographic",
        15=>"CT_PolarStereographic",
        16=>"CT_ObliqueStereographic",
        17=>"CT_Equirectangular",
        18=>"CT_CassiniSoldner",
        19=>"CT_Gnomonic",
        20=>"CT_MillerCylindrical",
        21=>"CT_Orthographic",
        22=>"CT_Polyconic",
        23=>"CT_Robinson",
        24=>"CT_Sinusoidal",
        25=>"CT_VanDerGrinten",
        26=>"CT_NewZealandMapGrid",
        27=>"CT_TransvMercator_SouthOriented"
    ];
    kw.insert(3075u16, proj_coord_trans_map);

    let proj_linear_units_map = hashmap![
        9001u16=>"Linear_Meter",
        9002=>"Linear_Foot",
        9003=>"Linear_Foot_US_Survey",
        9004=>"Linear_Foot_Modified_American",
        9005=>"Linear_Foot_Clarke",
        9006=>"Linear_Foot_Indian",
        9007=>"Linear_Link",
        9008=>"Linear_Link_Benoit",
        9009=>"Linear_Link_Sears",
        9010=>"Linear_Chain_Benoit",
        9011=>"Linear_Chain_Sears",
        9012=>"Linear_Yard_Sears",
        9013=>"Linear_Yard_Indian",
        9014=>"Linear_Fathom",
        9015=>"Linear_Mile_International_Nautical"
    ];
    kw.insert(3076u16, proj_linear_units_map);

    let vertical_cs_type_map = hashmap![
        1127=>"Canadian Geodetic Vertical Datum of 2013 (CGVD2013)",
        5001=>"VertCS_Airy_1830_ellipsoid",
        5002=>"VertCS_Airy_Modified_1849_ellipsoid",
        5003=>"VertCS_ANS_ellipsoid",
        5004=>"VertCS_Bessel_1841_ellipsoid",
        5005=>"VertCS_Bessel_Modified_ellipsoid",
        5006=>"VertCS_Bessel_Namibia_ellipsoid",
        5007=>"VertCS_Clarke_1858_ellipsoid",
        5008=>"VertCS_Clarke_1866_ellipsoid",
        5010=>"VertCS_Clarke_1880_Benoit_ellipsoid",
        5011=>"VertCS_Clarke_1880_IGN_ellipsoid",
        5012=>"VertCS_Clarke_1880_RGS_ellipsoid",
        5013=>"VertCS_Clarke_1880_Arc_ellipsoid",
        5014=>"VertCS_Clarke_1880_SGA_1922_ellipsoid",
        5015=>"VertCS_Everest_1830_1937_Adjustment_ellipsoid",
        5016=>"VertCS_Everest_1830_1967_Definition_ellipsoid",
        5017=>"VertCS_Everest_1830_1975_Definition_ellipsoid",
        5018=>"VertCS_Everest_1830_Modified_ellipsoid",
        5019=>"VertCS_GRS_1980_ellipsoid",
        5020=>"VertCS_Helmert_1906_ellipsoid",
        5021=>"VertCS_INS_ellipsoid",
        5022=>"VertCS_International_1924_ellipsoid",
        5023=>"VertCS_International_1967_ellipsoid",
        5024=>"VertCS_Krassowsky_1940_ellipsoid",
        5025=>"VertCS_NWL_9D_ellipsoid",
        5026=>"VertCS_NWL_10D_ellipsoid",
        5027=>"VertCS_Plessis_1817_ellipsoid",
        5028=>"VertCS_Struve_1860_ellipsoid",
        5029=>"VertCS_War_Office_ellipsoid",
        5030=>"VertCS_WGS_84_ellipsoid",
        5031=>"VertCS_GEM_10C_ellipsoid",
        5032=>"VertCS_OSU86F_ellipsoid",
        5033=>"VertCS_OSU91A_ellipsoid",
        5101=>"VertCS_Newlyn",
        5102=>"VertCS_North_American_Vertical_Datum_1929",
        5103=>"VertCS_North_American_Vertical_Datum_1988",
        5104=>"VertCS_Yellow_Sea_1956",
        5105=>"VertCS_Baltic_Sea",
        5106=>"VertCS_Caspian_Sea"
    ];
    kw.insert(4096u16, vertical_cs_type_map);

    let vertical_units_map = hashmap![
        9001u16=>"Linear_Meter",
        9002=>"Linear_Foot",
        9003=>"Linear_Foot_US_Survey",
        9004=>"Linear_Foot_Modified_American",
        9005=>"Linear_Foot_Clarke",
        9006=>"Linear_Foot_Indian",
        9007=>"Linear_Link",
        9008=>"Linear_Link_Benoit",
        9009=>"Linear_Link_Sears",
        9010=>"Linear_Chain_Benoit",
        9011=>"Linear_Chain_Sears",
        9012=>"Linear_Yard_Sears",
        9013=>"Linear_Yard_Indian",
        9014=>"Linear_Fathom",
        9015=>"Linear_Mile_International_Nautical"
    ];
    kw.insert(4099u16, vertical_units_map);

    kw
}

pub fn get_field_type_map() -> HashMap<u16, &'static str> {
    hashmap![
        1u16 => "DT_Byte",
        2u16 => "DT_ASCII",
        3u16 => "DT_Short",
        4u16 => "DT_Long",
        5u16 => "DT_Rational",
        6u16 => "DT_Sbyte",
        7u16 => "DT_Undefined",
        8u16 => "DT_Sshort",
        9u16 => "DT_Slong",
        10u16 => "DT_Srational",
        11u16 => "DT_Float",
        12u16 => "DT_Double",
        16u16 => "DT_Long8",
        17u16 => "DT_Slong8",
        18u16 => "DT_Ifd8"
    ]
}

#[derive(Default, Clone, Debug)]
pub struct TiffTag {
    pub name: String,
    pub code: u16,
}

impl TiffTag {
    pub fn get_name(self) -> String {
        self.name
    }

    pub fn new_unknown_tag() -> TiffTag {
        TiffTag {
            name: "Unknown".to_string(),
            code: 0,
        }
    }
}

impl fmt::Display for TiffTag {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = format!("Name: {}, Code: {}", self.name, self.code);
        write!(f, "{}", s)
    }
}
