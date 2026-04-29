import CoreLocation
import Foundation

class LocationFinder: NSObject, CLLocationManagerDelegate {
    let manager = CLLocationManager()
    var found = false

    override init() {
        super.init()
        manager.delegate = self
        manager.desiredAccuracy = kCLLocationAccuracyBest
        manager.startUpdatingLocation()
    }

    func locationManager(_ manager: CLLocationManager, didUpdateLocations locations: [CLLocation]) {
        guard !found, let loc = locations.last else { return }
        found = true
        manager.stopUpdatingLocation()

        // Reverse geocode to get city/region/country from coordinates
        CLGeocoder().reverseGeocodeLocation(loc) { placemarks, _ in
            let city    = placemarks?.first?.locality             ?? "?"
            let region  = placemarks?.first?.administrativeArea   ?? "?"
            let country = placemarks?.first?.country              ?? "?"
            let lat     = loc.coordinate.latitude
            let lon     = loc.coordinate.longitude
            print("RESULT: \(lat),\(lon),\(city),\(region),\(country)")
            exit(0)
        }
    }

    func locationManager(_ manager: CLLocationManager, didFailWithError error: Error) {
        print("ERROR: \(error.localizedDescription)")
        exit(1)
    }

    func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
        if manager.authorizationStatus == .denied || manager.authorizationStatus == .restricted {
            print("ERROR: Location access denied")
            exit(1)
        }
    }
}

let finder = LocationFinder()
RunLoop.main.run(until: Date(timeIntervalSinceNow: 15)) // 15 second timeout
print("ERROR: timeout")
exit(1)
