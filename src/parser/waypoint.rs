//! waypoint handles parsing of GPX-spec waypoints.

use crate::errors::*;
use std::io::Read;
use xml::reader::XmlEvent;
use error_chain::{bail, ensure};

use geo_types::Point;
use crate::parser::extensions;
use crate::parser::fix;
use crate::parser::link;
use crate::parser::string;
use crate::parser::time;
use crate::parser::verify_starting_tag;
use crate::parser::Context;

use crate::Waypoint;
use crate::GpxVersion;

/// consume consumes a GPX waypoint from the `reader` until it ends.
pub fn consume<R: Read>(context: &mut Context<R>, tagname: &'static str) -> Result<Waypoint> {
    let attributes = verify_starting_tag(context, tagname)?;

    // get required latitude and longitude
    let latitude = attributes
        .iter()
        .find(|attr| attr.name.local_name == "lat")
        .ok_or(ErrorKind::InvalidElementLacksAttribute(
            "latitude", "waypoint",
        ))?;

    let latitude: f64 = latitude
        .value
        .parse()
        .chain_err(|| "error while casting latitude to f64")?;

    let longitude = attributes
        .iter()
        .find(|attr| attr.name.local_name == "lon")
        .ok_or(ErrorKind::InvalidElementLacksAttribute(
            "longitude",
            "waypoint",
        ))?;

    let longitude: f64 = longitude
        .value
        .parse()
        .chain_err(|| "error while casting longitude to f64")?;

    let mut waypoint: Waypoint = Waypoint::new(Point::new(longitude, latitude));

    loop {
        let next_event = {
            if let Some(next) = context.reader.peek() {
                match next {
                    Ok(n) => n,
                    Err(_) => bail!("error while parsing waypoint event"),
                }
            } else {
                break;
            }
        };

        match next_event {
            XmlEvent::StartElement { ref name, .. } => {
                match name.local_name.as_ref() {
                    "ele" => {
                        // Cast the elevation to an f64, from a string.
                        waypoint.elevation = Some(
                            string::consume(context, "ele", false)?
                                .parse()
                                .chain_err(|| "error while casting elevation to f64")?,
                        )
                    }
                    "speed" if context.version == GpxVersion::Gpx10 => {
                        // Speed is from GPX 1.0
                        waypoint.speed = Some(
                            string::consume(context, "speed", false)?
                                .parse()
                                .chain_err(|| "error while casting speed to f64")?,
                        );
                    }
                    "time" => waypoint.time = Some(time::consume(context)?),
                    "name" => waypoint.name = Some(string::consume(context, "name", false)?),
                    "cmt" => waypoint.comment = Some(string::consume(context, "cmt", true)?),
                    "desc" => waypoint.description = Some(string::consume(context, "desc", true)?),
                    "src" => waypoint.source = Some(string::consume(context, "src", true)?),
                    "link" => waypoint.links.push(link::consume(context)?),
                    "sym" => waypoint.symbol = Some(string::consume(context, "sym", false)?),
                    "type" => waypoint._type = Some(string::consume(context, "type", false)?),

                    // Optional accuracy information
                    "fix" => waypoint.fix = Some(fix::consume(context)?),
                    "geoidheight" => {
                        waypoint.geoidheight = Some(
                            string::consume(context, "geoidheight", false)?
                                .parse()
                                .chain_err(|| "error while casting geoid (geoidheight) to f64")?,
                        )
                    }
                    "sat" => {
                        waypoint.sat =
                            Some(string::consume(context, "sat", false)?.parse().chain_err(
                                || "error while casting number of satellites (sat) to u64",
                            )?)
                    }
                    "hdop" => waypoint.hdop = Some(
                        string::consume(context, "hdop", false)?
                            .parse()
                            .chain_err(|| {
                                "error while casting horizontal dilution of precision (hdop) to f64"
                            })?,
                    ),
                    "vdop" => waypoint.vdop = Some(
                        string::consume(context, "vdop", false)?
                            .parse()
                            .chain_err(|| {
                                "error while casting vertical dilution of precision (vdop) to f64"
                            })?,
                    ),
                    "pdop" => waypoint.pdop = Some(
                        string::consume(context, "pdop", false)?
                            .parse()
                            .chain_err(|| {
                                "error while casting position dilution of precision (pdop) to f64"
                            })?,
                    ),
                    "ageofgpsdata" => {
                        waypoint.age = Some(
                            string::consume(context, "ageofgpsdata", false)?
                                .parse()
                                .chain_err(|| "error while casting age of GPS data to f64")?,
                        )
                    }
                    "dgpsid" => {
                        waypoint.dgpsid = Some(
                            string::consume(context, "dgpsid", false)?
                                .parse()
                                .chain_err(|| "error while casting DGPS station ID to u16")?,
                        )
                    }

                    // Finally the GPX 1.1 extensions
                    "extensions" => extensions::consume(context)?,
                    child => {
                        bail!(ErrorKind::InvalidChildElement(
                            String::from(child),
                            "waypoint"
                        ));
                    }
                }
            }
            XmlEvent::EndElement { ref name } => {
                ensure!(
                    name.local_name == tagname,
                    ErrorKind::InvalidClosingTag(name.local_name.clone(), "waypoint")
                );
                context.reader.next(); //consume the end tag
                return Ok(waypoint);
            }
            _ => {
                context.reader.next(); //consume and ignore this event
            }
        }
    }

    bail!(ErrorKind::MissingClosingTag("waypoint"));
}

#[cfg(test)]
mod tests {
    use geo_types::Point;
    use std::io::BufReader;

    use super::consume;
    use crate::Fix;
    use crate::GpxVersion;

    #[test]
    fn consume_waypoint() {
        let waypoint = consume!(
            "
            <wpt lon=\"-77.0365\" lat=\"38.8977\">
                <name>The White House</name>
                <cmt>This is a comment about the white house.</cmt>
                <desc>The white house is very nice!</desc>
                <src>Garmin eTrex</src>
                <type>waypoint classification</type>
                <ele>4608.12</ele>
                <fix>dgps</fix>
                <sat>4</sat>
                <hdop>6.058</hdop>
                <speed>0.0000</speed>
            </wpt>
            ",
            GpxVersion::Gpx10,
            "wpt"
        );

        assert!(waypoint.is_ok());

        let waypoint = waypoint.unwrap();

        assert_eq!(waypoint.point(), Point::new(-77.0365, 38.8977));
        assert_eq!(waypoint.name.unwrap(), "The White House");
        assert_eq!(
            waypoint.comment.unwrap(),
            "This is a comment about the white house."
        );
        assert_eq!(
            waypoint.description.unwrap(),
            "The white house is very nice!"
        );
        assert_eq!(waypoint.source.unwrap(), "Garmin eTrex");
        assert_eq!(waypoint._type.unwrap(), "waypoint classification");
        assert_eq!(waypoint.elevation.unwrap(), 4608.12);
        assert_eq!(waypoint.fix.unwrap(), Fix::DGPS);
        assert_eq!(waypoint.sat.unwrap(), 4);
        assert_eq!(waypoint.hdop.unwrap(), 6.058);
    }

    #[test]
    fn consume_empty() {
        let waypoint = consume!(
            "<trkpt lat=\"2.345\" lon=\"1.234\"></trkpt>",
            GpxVersion::Gpx11,
            "trkpt"
        );

        assert!(waypoint.is_ok());
        let waypoint = waypoint.unwrap();

        assert_eq!(waypoint.point(), Point::new(1.234, 2.345));
        assert_eq!(waypoint.point().lng(), 1.234);
        assert_eq!(waypoint.point().lat(), 2.345);
    }

    #[test]
    fn consume_bad_waypoint() {
        let waypoint = consume!(
            "<wpt lat=\"32.4\" lon=\"notanumber\"></wpt>",
            GpxVersion::Gpx11,
            "wpt"
        );

        assert!(waypoint.is_err());
    }
}
