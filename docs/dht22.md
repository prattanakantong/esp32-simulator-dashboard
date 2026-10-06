<!-- Summary compiled by the project author. Verify against the manufacturer datasheet before relying on exact numbers. -->

# DHT22 (AM2302) temperature and humidity sensor

## DHT22 overview

The DHT22, also sold as AM2302, is a low-cost digital sensor that measures temperature and relative humidity. Both values are returned together in a single reading over a single-wire digital interface, so it needs only one data pin on the microcontroller.

## DHT22 measurement range and accuracy

Typical datasheet values: humidity range 0 to 100 %RH with accuracy of about plus or minus 2 %RH (up to 5 %RH in the worst case); temperature range minus 40 to plus 80 degrees Celsius with accuracy of about plus or minus 0.5 degrees Celsius. Resolution is 0.1 for both humidity and temperature. A reading outside these ranges, or a sudden jump of several degrees between two consecutive samples, is more likely a wiring or sensor fault than a real change.

## DHT22 sampling rate

The DHT22 can be read at most once every 2 seconds (0.5 Hz). Reading more often returns stale values or errors. In the Arduino DHT library a failed read shows up as NaN. Publishing a reading every 2 seconds is therefore the fastest sensible rate for this sensor.

## DHT22 power and wiring

The sensor is commonly powered at 3.3 to 5 V (datasheets list a somewhat wider supply range). The 4-pin version has pins VCC, DATA, an unused pin and GND, and needs a pull-up resistor of roughly 4.7 to 10 kilohms between DATA and VCC. Many 3-pin breakout modules already include the pull-up. When powered from 3.3 V the data line is 3.3 V logic, which is safe for an ESP32. In this project's reference firmware the data pin is GPIO 4.

## DHT22 troubleshooting NaN readings

Common causes of NaN or failed reads: missing pull-up resistor on a bare 4-pin sensor; wrong data pin or wrong sensor type in the code (DHT11 instead of DHT22); reading faster than once every 2 seconds; loose wiring or a long cable; unstable power supply. Check wiring and pull-up first, then the read interval.

## DHT22 compared with DHT11

The DHT11 is cheaper but less capable: humidity 20 to 80 %RH with about 5 %RH accuracy, temperature 0 to 50 degrees Celsius with about 2 degrees accuracy, and whole-number resolution. The DHT22 has a wider range, better accuracy and 0.1 resolution, so it is the better choice for monitoring.

## DHT22 source and reliability

This summary is compiled by the project author from manufacturer datasheets and common usage. Values are typical, not guaranteed. For design decisions, consult the official datasheet.
